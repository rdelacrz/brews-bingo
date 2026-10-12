//! Fresh private recipient eligibility and minimal bounded discovery. No Users projection.
use super::*;
use crate::db::SqlValue;
use brews_contracts::games::HostTransferTarget;
use brews_domain::accounts::{AccountRole, AccountStatus};
use brews_domain::ids::AccountId;
#[derive(Debug)]
pub struct HostRecipientAuthority {
    account: AccountId,
    epoch: i64,
    checked: i64,
    binding: Option<crate::game::HostTransferWork>,
}
impl HostRecipientAuthority {
    pub fn account_id(&self) -> AccountId {
        self.account
    }
    pub fn credential_epoch(&self) -> i64 {
        self.epoch
    }
    pub fn checked_at(&self) -> i64 {
        self.checked
    }
    pub(crate) fn matches_transfer(&self, work: &crate::game::HostTransferWork) -> bool {
        self.binding
            .as_ref()
            .is_some_and(|binding| binding.same_binding(work))
    }
    pub(crate) fn bind_transfer(
        mut self,
        work: &crate::game::HostTransferWork,
    ) -> Result<Self, AuthError> {
        if self.account != work.target || work.phase != crate::game::TransferPhase::Prepared {
            return Err(AuthError::InvalidInput);
        }
        self.binding = Some(work.clone());
        Ok(self)
    }
    pub(crate) fn from_trusted_peer(
        account: AccountId,
        epoch: i64,
        checked: i64,
    ) -> Result<Self, AuthError> {
        if !(0..=crate::limits::JS_SAFE_INTEGER_MAX).contains(&epoch)
            || !(1..=crate::limits::JS_SAFE_INTEGER_MAX).contains(&checked)
        {
            return Err(AuthError::InvalidInput);
        }
        Ok(Self {
            account,
            epoch,
            checked,
            binding: None,
        })
    }
}
impl<D: Database, R: Runtime> AuthService<'_, D, R> {
    /// Recipient is checked together with the actor in the serialized Accounts owner.
    pub fn authorize_host_recipient(
        &self,
        token: &str,
        target: AccountId,
    ) -> Result<(GameAccountAuthority, HostRecipientAuthority), AuthError> {
        self.transaction(|| {
            let session = self.session(token)?.ok_or(AuthError::Unauthorized)?;
            let authority = self.game_authority(&session)?;
            let recipient = self.account(target)?.ok_or(AuthError::Conflict)?;
            if recipient.role != AccountRole::Host
                || recipient.status != AccountStatus::Verified
                || recipient.disabled.is_some()
                || recipient.verifier.is_none()
            {
                return Err(AuthError::Conflict);
            }
            let proof =
                HostRecipientAuthority::from_trusted_peer(target, recipient.epoch, self.now()?)?;
            Ok((authority, proof))
        })
    }
    pub fn authorize_host_transfer_recipient(
        &self,
        token: &str,
        work: &crate::game::HostTransferWork,
    ) -> Result<(GameAccountAuthority, HostRecipientAuthority), AuthError> {
        let (actor, recipient) = self.authorize_host_recipient(token, work.target)?;
        if actor.account_id() != work.actor {
            return Err(AuthError::Unauthorized);
        }
        Ok((actor, recipient.bind_transfer(work)?))
    }
    pub fn eligible_host_targets(
        &self,
        token: &str,
        after: Option<AccountId>,
        limit: u32,
    ) -> Result<(GameAccountAuthority, Vec<HostTransferTarget>), AuthError> {
        if !(1..=brews_contracts::games::HOST_TRANSFER_TARGET_LIMIT_MAX).contains(&limit) {
            return Err(AuthError::InvalidInput);
        }
        self.transaction(||{
            let session=self.session(token)?.ok_or(AuthError::Unauthorized)?;
            let authority=self.game_authority(&session)?;
            let cursor=after.map_or(SqlValue::Null,|id|SqlValue::Text(id.to_string()));
            let rows=self.db.query("SELECT account_id,username FROM accounts WHERE role=? AND status=? AND disabled_at IS NULL AND verifier IS NOT NULL AND (? IS NULL OR account_id>?) ORDER BY account_id LIMIT ?",&[SqlValue::Text(AccountRole::Host.to_string()),SqlValue::Text(AccountStatus::Verified.to_string()),cursor.clone(),cursor,SqlValue::Integer(i64::from(limit)+1)])?;
            let targets=rows.iter().map(|row|{
                let target=HostTransferTarget {account_id:super::records::text(row,0)?.parse().map_err(|_|AuthError::Storage)?,username:super::records::text(row,1)?.to_owned()};
                if brews_domain::accounts::validate_username(&target.username).map_err(|_|AuthError::Storage)?!=target.username {return Err(AuthError::Storage);}
                Ok(target)
            }).collect::<Result<Vec<_>,AuthError>>()?;
            Ok((authority,targets))
        })
    }
}
