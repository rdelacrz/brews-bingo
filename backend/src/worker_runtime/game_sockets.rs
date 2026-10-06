//! Allocation-conscious hibernating transport. Attachments are bounded lookup hints only.
use super::{
    database::OwnerDatabase,
    game_accounts::AccountConnectionIdentity,
    game_object::{GameObject, peer_error},
    game_peers,
    game_wire::{GameIngress, ViewSelector},
    runtime::WorkerRuntime,
};
use crate::{
    auth::Runtime,
    db::{
        game::GameService,
        game_delivery::{DeliveryBinding, DeliveryError, DeliveryService},
    },
    game::{AccountConnectionGrant, ConnectionGrant, GameError},
};
use brews_contracts::games::{DeliveryId, GameView, SnapshotAck, SnapshotFrame, SnapshotKind};
use brews_domain::ids::{AccountId, ConnectionId, GameId, PlayerId, SessionId};
use js_sys::{Array, Function, Reflect};
use serde::{Deserialize, Serialize};
use wasm_bindgen::{JsCast, JsValue};
use worker::{Response, WebSocket, WebSocketIncomingMessage, WebSocketPair};

const ATTACHMENT_MAX_BYTES: usize = 512;
const SOCKET_LIMIT: usize = 100;
#[derive(Clone, Copy)]
#[repr(u16)]
pub(super) enum CloseCode {
    Normal = 1000,
    Policy = 1008,
    Credit = 1009,
    Resynchronize = 1013,
}
impl CloseCode {
    pub(super) const fn value(self) -> u16 {
        self as u16
    }
}
const SIZE_PLACEHOLDER: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub(super) enum Viewer {
    Account(AccountId),
    Player(PlayerId),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Attachment {
    version: u8,
    pub(super) game_id: GameId,
    pub(super) connection_id: ConnectionId,
    pub(super) session_id: SessionId,
    pub(super) viewer: Viewer,
    pub(super) authentication_epoch: i64,
    pub(super) session_expires_at: i64,
    view: String,
    last_sent_revision: Option<u64>,
}
impl Attachment {
    fn player(grant: &ConnectionGrant) -> Self {
        Self {
            version: 1,
            game_id: grant.game_id(),
            connection_id: grant.connection_id(),
            session_id: grant.session_id(),
            viewer: Viewer::Player(grant.player_id()),
            authentication_epoch: grant.epoch(),
            session_expires_at: grant.expires_at(),
            view: grant.player_id().to_string(),
            last_sent_revision: None,
        }
    }
    fn account(grant: &AccountConnectionGrant) -> Self {
        Self {
            version: 1,
            game_id: grant.game_id(),
            connection_id: grant.connection_id(),
            session_id: grant.session_id(),
            viewer: Viewer::Account(grant.account_id()),
            authentication_epoch: grant.epoch(),
            session_expires_at: grant.expires_at(),
            view: "host".into(),
            last_sent_revision: None,
        }
    }
    pub(super) fn read(ws: &WebSocket) -> Result<Self, GameError> {
        let text = ws
            .deserialize_attachment::<String>()
            .map_err(|_| GameError::Unauthorized)?
            .ok_or(GameError::Unauthorized)?;
        if text.len() > ATTACHMENT_MAX_BYTES || !text.starts_with('{') {
            return Err(GameError::Unauthorized);
        }
        let value: Self = serde_json::from_str(&text).map_err(|_| GameError::Unauthorized)?;
        if value.version != 1
            || !(0..=crate::limits::JS_SAFE_INTEGER_MAX).contains(&value.authentication_epoch)
            || !(1..=crate::limits::JS_SAFE_INTEGER_MAX).contains(&value.session_expires_at)
            || value
                .last_sent_revision
                .is_some_and(|r| r > crate::limits::JS_SAFE_INTEGER_MAX as u64)
            || match value.viewer {
                Viewer::Account(_) => value.view != "host",
                Viewer::Player(p) => value.view != p.to_string(),
            }
        {
            return Err(GameError::Unauthorized);
        }
        Ok(value)
    }
    fn write(&self, ws: &WebSocket) -> Result<(), GameError> {
        let text = serde_json::to_string(self).map_err(|_| GameError::Storage)?;
        if text.len() > ATTACHMENT_MAX_BYTES {
            return Err(GameError::Storage);
        }
        ws.serialize_attachment(text)
            .map_err(|_| GameError::Storage)
    }
    pub(super) fn binding(&self) -> DeliveryBinding {
        DeliveryBinding {
            connection_id: self.connection_id,
            session_id: self.session_id,
            auth_epoch: self.authentication_epoch as u64,
            expires_at: self.session_expires_at,
        }
    }
    pub(super) fn identity(&self) -> Result<AccountConnectionIdentity, GameError> {
        let Viewer::Account(account_id) = self.viewer else {
            return Err(GameError::Unauthorized);
        };
        Ok(AccountConnectionIdentity {
            account_id,
            session_id: self.session_id,
            game_id: self.game_id,
            connection_id: self.connection_id,
            epoch: self.authentication_epoch,
            expires: self.session_expires_at,
        })
    }
    fn account_grant(&self) -> Result<AccountConnectionGrant, GameError> {
        let identity = self.identity()?;
        Ok(AccountConnectionGrant {
            game: identity.game_id,
            session: identity.session_id,
            account: identity.account_id,
            connection: identity.connection_id,
            epoch: identity.epoch,
            expires: identity.expires,
        })
    }
    fn same_authority(&self, other: &Self) -> bool {
        self.version == other.version
            && self.game_id == other.game_id
            && self.connection_id == other.connection_id
            && self.session_id == other.session_id
            && self.viewer == other.viewer
            && self.authentication_epoch == other.authentication_epoch
            && self.session_expires_at == other.session_expires_at
            && self.view == other.view
    }
    fn matches_player(&self, grant: &ConnectionGrant) -> bool {
        self.game_id == grant.game_id()
            && self.session_id == grant.session_id()
            && self.connection_id == grant.connection_id()
            && self.viewer == Viewer::Player(grant.player_id())
            && self.authentication_epoch == grant.epoch()
            && self.session_expires_at == grant.expires_at()
    }
}
fn delivery_error(error: DeliveryError) -> GameError {
    match error {
        DeliveryError::Storage | DeliveryError::Clock | DeliveryError::Random => GameError::Storage,
        DeliveryError::Capacity => GameError::Capacity,
        DeliveryError::Expired => GameError::Expired,
        DeliveryError::BindingMismatch => GameError::Unauthorized,
        DeliveryError::InvalidInput
        | DeliveryError::UnknownDelivery
        | DeliveryError::RevisionMismatch => GameError::InvalidInput,
    }
}
fn sdk_error(_: JsValue) -> GameError {
    GameError::Storage
}
fn method(target: &JsValue, name: &str) -> Result<Function, GameError> {
    Reflect::get(target, &JsValue::from_str(name))
        .map_err(sdk_error)?
        .dyn_into::<Function>()
        .map_err(sdk_error)
}
pub(super) fn ready_state(ws: &WebSocket) -> Result<u16, GameError> {
    let value = Reflect::get(ws.as_ref().as_ref(), &JsValue::from_str("readyState"))
        .map_err(sdk_error)?
        .as_f64()
        .ok_or(GameError::Storage)?;
    if value.fract() != 0.0 || !(0.0..=3.0).contains(&value) {
        return Err(GameError::Storage);
    }
    Ok(value as u16)
}
impl GameObject {
    pub(super) fn owns_game(&self, id: GameId) -> Result<bool, GameError> {
        let raw = self.state.as_raw().as_ref();
        let owner = Reflect::get(raw, &JsValue::from_str("id")).map_err(sdk_error)?;
        let name = Reflect::get(&owner, &JsValue::from_str("name")).map_err(sdk_error)?;
        Ok(name.as_string().as_deref() == Some(id.to_string().as_str()))
    }
    pub(super) fn sockets(&self) -> Result<Vec<WebSocket>, GameError> {
        // Avoid worker::State convenience methods which unwrap SDK failures.
        let raw = self.state.as_raw().as_ref();
        let value = method(raw, "getWebSockets")?
            .call0(raw)
            .map_err(sdk_error)?;
        if !Array::is_array(&value) {
            return Err(GameError::Storage);
        }
        let array = Array::from(&value);
        if array.length() > SOCKET_LIMIT as u32 {
            // Drain at most one normal-sized batch. Until enumeration is complete,
            // fail closed: truncated handles must never establish an absence/close ACK.
            for value in array.iter().take(SOCKET_LIMIT) {
                let ws = value
                    .dyn_into::<worker::web_sys::WebSocket>()
                    .map(WebSocket::from)
                    .map_err(sdk_error)?;
                if ready_state(&ws)? == 1 {
                    ws.close(
                        Some(CloseCode::Resynchronize.value()),
                        Some("registry resynchronize"),
                    )
                    .map_err(|_| GameError::Storage)?;
                }
            }
            return Err(GameError::Storage);
        }
        array
            .iter()
            .map(|v| {
                v.dyn_into::<worker::web_sys::WebSocket>()
                    .map(WebSocket::from)
                    .map_err(sdk_error)
            })
            .collect()
    }
    fn accept_socket(&self, ws: &WebSocket) -> Result<(), GameError> {
        // No await separates native capacity inspection from acceptance.
        if self.sockets()?.len() >= SOCKET_LIMIT {
            return Err(GameError::Capacity);
        }
        let raw = self.state.as_raw().as_ref();
        method(raw, "acceptWebSocket")?
            .call1(raw, ws.as_ref().as_ref())
            .map_err(sdk_error)?;
        if ready_state(ws)? != 1 {
            return Err(GameError::Storage);
        }
        Ok(())
    }
    pub(super) fn observed(
        &self,
        service: &GameService<'_, OwnerDatabase, WorkerRuntime>,
    ) -> Result<Vec<ConnectionId>, GameError> {
        let mut out = Vec::with_capacity(SOCKET_LIMIT);
        for ws in self.sockets()? {
            if ready_state(&ws)? != 1 {
                continue;
            }
            let hint = match Attachment::read(&ws) {
                Ok(hint) => hint,
                Err(_) => {
                    ws.close(
                        Some(CloseCode::Policy.value()),
                        Some("invalid connection metadata"),
                    )
                    .map_err(|_| GameError::Storage)?;
                    continue;
                }
            };
            if matches!(hint.viewer, Viewer::Player(_)) {
                match service.player_connection(hint.connection_id) {
                    Ok(Some(grant)) if hint.matches_player(&grant) => out.push(hint.connection_id),
                    Ok(_)
                    | Err(GameError::Unauthorized | GameError::Expired | GameError::NotFound) => {
                        ws.close(Some(CloseCode::Policy.value()), Some("session unavailable"))
                            .map_err(|_| GameError::Storage)?;
                    }
                    Err(error) => return Err(error),
                }
            }
        }
        Ok(out)
    }
    pub(super) async fn upgrade(&self, ingress: &GameIngress) -> Result<Response, GameError> {
        let db = self.database()?;
        let rt = WorkerRuntime;
        let service = self.service(&db, &rt)?;
        let token = ingress
            .session_token
            .as_deref()
            .ok_or(GameError::Unauthorized)?;
        let id = ingress.game_id.ok_or(GameError::InvalidInput)?;
        if service.directory_projection()?.game_id() != id {
            return Err(GameError::Conflict);
        }
        if self.sockets()?.len() >= SOCKET_LIMIT {
            return Err(GameError::Capacity);
        }
        let connection = connection_id()?;
        let pair = WebSocketPair::new().map_err(|_| GameError::Storage)?;
        let hint = match ingress.view {
            Some(ViewSelector::Player) => {
                let grant = service.prepare_player_connection(token, connection)?;
                self.persist(&service, &db).await?;
                // Owner revalidates the original durable session at the acceptance cut.
                service.player_view(token)?;
                let hint = Attachment::player(&grant);
                hint.write(&pair.server)?;
                service.validate_prepared_player_connection(&grant)?;
                self.accept_socket(&pair.server)?;
                if let Err(error) = service.accept_player_connection(&grant) {
                    let _ = pair.server.close(
                        Some(CloseCode::Resynchronize.value()),
                        Some("resynchronize"),
                    );
                    return Err(error);
                }
                hint
            }
            Some(ViewSelector::Account) => {
                self.persist(&service, &db).await?;
                let authority =
                    game_peers::register_account_connection(&self.env, token, id, connection)
                        .await
                        .map_err(peer_error)?;
                let identity =
                    AccountConnectionIdentity::from_authority(&authority, id, connection);
                let grant = service.prepare_account_connection(authority, connection)?;
                self.persist(&service, &db).await?;
                let authority = game_peers::authorize_account_connection(&self.env, &identity)
                    .await
                    .map_err(peer_error)?;
                let hint = Attachment::account(&grant);
                hint.write(&pair.server)?;
                service.validate_prepared_account_connection(&grant, &authority)?;
                self.accept_socket(&pair.server)?;
                if let Err(error) = service.accept_account_connection(&grant, authority) {
                    let _ = pair.server.close(
                        Some(CloseCode::Resynchronize.value()),
                        Some("resynchronize"),
                    );
                    return Err(error);
                }
                hint
            }
            None => return Err(GameError::InvalidInput),
        };
        // Only successful native acceptance and owner commit fence the predecessor.
        for old in self.sockets()? {
            if old == pair.server {
                continue;
            }
            if let Ok(old_hint) = Attachment::read(&old)
                && old_hint.session_id == hint.session_id
                && old_hint.viewer == hint.viewer
            {
                old.close(Some(CloseCode::Policy.value()), Some("connection replaced"))
                    .map_err(|_| GameError::Storage)?;
                DeliveryService::new(&db, &rt)
                    .map_err(|_| GameError::Storage)?
                    .retire_connection(old_hint.binding())
                    .map_err(|_| GameError::Storage)?;
            }
        }
        DeliveryService::new(&db, &rt)
            .map_err(|_| GameError::Storage)?
            .register(hint.binding())
            .map_err(|_| GameError::Storage)?;
        self.persist(&service, &db).await?;
        if let Err(error) = self.send_snapshot(&pair.server, &service, &db).await {
            self.retire_socket(&pair.server, &service, &db, CloseCode::Resynchronize)
                .await?;
            return Err(error);
        }
        self.broadcast(&service, &db).await?;
        Response::from_websocket(pair.client).map_err(|_| GameError::Storage)
    }
    async fn connection_view(
        &self,
        hint: &Attachment,
        service: &GameService<'_, OwnerDatabase, WorkerRuntime>,
        db: &OwnerDatabase,
    ) -> Result<GameView, GameError> {
        match hint.viewer {
            Viewer::Player(_) => {
                let grant = service
                    .player_connection(hint.connection_id)?
                    .ok_or(GameError::Unauthorized)?;
                if !hint.matches_player(&grant) {
                    return Err(GameError::Unauthorized);
                }
                service.player_connection_view(&grant)
            }
            Viewer::Account(_) => {
                self.persist(service, db).await?;
                let authority =
                    game_peers::authorize_account_connection(&self.env, &hint.identity()?)
                        .await
                        .map_err(peer_error)?;
                service.reconcile_connections(&self.observed(service)?)?;
                service.account_connection_view(&hint.account_grant()?, authority)
            }
        }
    }
    pub(super) async fn send_snapshot(
        &self,
        ws: &WebSocket,
        service: &GameService<'_, OwnerDatabase, WorkerRuntime>,
        db: &OwnerDatabase,
    ) -> Result<(), GameError> {
        if ready_state(ws)? != 1 {
            return Err(GameError::Unauthorized);
        }
        let hint = Attachment::read(ws)?;
        if !self.sending.borrow_mut().insert(hint.connection_id) {
            // Concurrent protected output must resynchronize, never silently drop a revision.
            return Err(GameError::Conflict);
        }
        struct InFlight<'a> {
            owner: &'a GameObject,
            id: ConnectionId,
        }
        impl Drop for InFlight<'_> {
            fn drop(&mut self) {
                self.owner.sending.borrow_mut().remove(&self.id);
            }
        }
        let _flight = InFlight {
            owner: self,
            id: hint.connection_id,
        };
        let view = self.connection_view(&hint, service, db).await?;
        let current_hint = Attachment::read(ws)?;
        if !hint.same_authority(&current_hint) {
            return Err(GameError::Conflict);
        }
        if current_hint
            .last_sent_revision
            .is_some_and(|r| r >= view.view_revision())
        {
            return Ok(());
        }
        let mut frame = SnapshotFrame {
            version: 1,
            kind: SnapshotKind::Snapshot,
            game_id: hint.game_id,
            view_revision: view.view_revision(),
            connection_id: hint.connection_id,
            session_expires_at: hint.session_expires_at,
            view,
            delivery_id: SIZE_PLACEHOLDER
                .parse::<DeliveryId>()
                .map_err(|_| GameError::Storage)?,
        };
        let original_bytes = frame.encode_json().map_err(|_| GameError::Storage)?;
        let size = original_bytes.len();
        let delivery = DeliveryService::new(db, &WorkerRuntime).map_err(|_| GameError::Storage)?;
        let reserved = delivery
            .reserve_delivery(hint.binding(), frame.view_revision, size)
            .map_err(delivery_error)?;
        self.persist(service, db).await?;
        // The final peer proof precedes a new local snapshot read, never an old captured view.
        let final_view = self.connection_view(&hint, service, db).await?;
        if ready_state(ws)? != 1 || final_view.view_revision() != frame.view_revision {
            return Err(GameError::Conflict);
        }
        frame.view = final_view;
        if frame.encode_json().map_err(|_| GameError::Storage)? != original_bytes {
            return Err(GameError::Conflict);
        }
        let mut current = Attachment::read(ws)?;
        if !hint.same_authority(&current)
            || current
                .last_sent_revision
                .is_some_and(|r| r >= frame.view_revision)
        {
            return Err(GameError::Conflict);
        }
        frame.delivery_id = reserved.delivery_id;
        let bytes = frame.encode_json().map_err(|_| GameError::Storage)?;
        if bytes.len() != size {
            return Err(GameError::Storage);
        }
        ws.send_with_str(std::str::from_utf8(&bytes).map_err(|_| GameError::Storage)?)
            .map_err(|_| GameError::Storage)?;
        current.last_sent_revision = Some(frame.view_revision);
        current.write(ws)
    }
    pub(super) async fn broadcast(
        &self,
        service: &GameService<'_, OwnerDatabase, WorkerRuntime>,
        db: &OwnerDatabase,
    ) -> Result<(), GameError> {
        service.reconcile_connections(&self.observed(service)?)?;
        self.persist(service, db).await?;
        for ws in self.sockets()? {
            if ready_state(&ws)? == 2 {
                // A real close handshake is not a close ACK; the combined wake retains it.
                continue;
            }
            if ready_state(&ws)? != 1 {
                self.retire_socket(&ws, service, db, CloseCode::Resynchronize)
                    .await?;
            } else if let Err(error) = self.send_snapshot(&ws, service, db).await {
                let code = match error {
                    GameError::Capacity => CloseCode::Credit,
                    GameError::Unauthorized
                    | GameError::Expired
                    | GameError::NotFound
                    | GameError::Forbidden => CloseCode::Policy,
                    _ => CloseCode::Resynchronize,
                };
                self.retire_socket(&ws, service, db, code).await?;
            }
        }
        self.persist(service, db).await
    }
    pub(super) async fn retire_socket(
        &self,
        ws: &WebSocket,
        service: &GameService<'_, OwnerDatabase, WorkerRuntime>,
        db: &OwnerDatabase,
        code: CloseCode,
    ) -> Result<(), GameError> {
        if ready_state(ws)? != 3 {
            ws.close(Some(code.value()), Some("resynchronize"))
                .map_err(|_| GameError::Storage)?;
        }
        let hint = Attachment::read(ws).ok();
        if let Some(hint) = &hint {
            match hint.viewer {
                Viewer::Player(_) => match service.player_connection(hint.connection_id) {
                    Ok(Some(grant)) if hint.matches_player(&grant) => {
                        service.disconnect(&grant)?;
                    }
                    Ok(_)
                    | Err(GameError::NotFound | GameError::Expired | GameError::Unauthorized) => {}
                    Err(error) => return Err(error),
                },
                Viewer::Account(_) => {
                    let identity = hint.identity()?;
                    let work = crate::auth::GameSocketCloseWork::from_trusted_peer(
                        identity.account_id,
                        identity.session_id,
                        identity.game_id,
                        identity.connection_id,
                        connection_id()?
                            .to_string()
                            .parse()
                            .map_err(|_| GameError::Storage)?,
                        identity.epoch,
                        identity.expires,
                        WorkerRuntime.now_ms(),
                    )
                    .map_err(|_| GameError::Storage)?;
                    service.close_account_connection(&work)?;
                }
            }
        }
        service.reconcile_connections(&self.observed(service)?)?;
        self.persist(service, db).await?;
        if ready_state(ws)? != 3 {
            return Err(GameError::Storage);
        }
        if let Some(hint) = &hint {
            DeliveryService::new(db, &WorkerRuntime)
                .map_err(|_| GameError::Storage)?
                .retire_connection(hint.binding())
                .map_err(|_| GameError::Storage)?;
            self.persist(service, db).await?;
            if matches!(hint.viewer, Viewer::Account(_)) {
                match game_peers::unregister_account_connection(&self.env, &hint.identity()?).await
                {
                    Ok(())
                    | Err(
                        game_peers::GamePeerError::Conflict
                        | game_peers::GamePeerError::Unauthorized,
                    ) => {}
                    Err(error) => return Err(peer_error(error)),
                }
            }
        }
        self.persist(service, db).await
    }
    pub(super) async fn incoming(
        &self,
        ws: WebSocket,
        message: WebSocketIncomingMessage,
    ) -> Result<(), GameError> {
        let db = self.database()?;
        let service = self.service(&db, &WorkerRuntime)?;
        let oversized = match &message {
            WebSocketIncomingMessage::String(text) => {
                text.len() > brews_contracts::games::SNAPSHOT_ACK_MAX_BYTES
            }
            WebSocketIncomingMessage::Binary(bytes) => {
                bytes.len() > brews_contracts::games::SNAPSHOT_ACK_MAX_BYTES
            }
        };
        let bytes = match &message {
            WebSocketIncomingMessage::String(text) if !oversized => Some(text.as_bytes()),
            _ => None,
        };
        let result = async {
            let bytes = bytes.ok_or(GameError::InvalidInput)?;
            let ack = SnapshotAck::decode_json(bytes).map_err(|_| GameError::InvalidInput)?;
            if ready_state(&ws)? != 1 {
                return Err(GameError::Unauthorized);
            }
            let hint = Attachment::read(&ws)?;
            self.connection_view(&hint, &service, &db).await?;
            DeliveryService::new(&db, &WorkerRuntime)
                .map_err(|_| GameError::Storage)?
                .acknowledge(hint.binding(), &ack)
                .map_err(delivery_error)?;
            self.persist(&service, &db).await
        }
        .await;
        if result.is_err() {
            self.retire_socket(
                &ws,
                &service,
                &db,
                match result {
                    Err(
                        GameError::Storage | GameError::RandomUnavailable | GameError::Conflict,
                    ) => CloseCode::Resynchronize,
                    _ if oversized => CloseCode::Credit,
                    _ => CloseCode::Policy,
                },
            )
            .await?;
        }
        Ok(())
    }
}
fn connection_id() -> Result<ConnectionId, GameError> {
    let now = WorkerRuntime.now_ms();
    if !(0..=crate::limits::JS_SAFE_INTEGER_MAX).contains(&now) {
        return Err(GameError::Storage);
    }
    let mut bytes = [0u8; 16];
    WorkerRuntime
        .fill_random(&mut bytes)
        .map_err(|_| GameError::RandomUnavailable)?;
    bytes[..6].copy_from_slice(&now.to_be_bytes()[2..]);
    bytes[6] = (bytes[6] & 15) | 112;
    bytes[8] = (bytes[8] & 63) | 128;
    uuid::Uuid::from_bytes(bytes)
        .try_into()
        .map_err(|_| GameError::Storage)
}
