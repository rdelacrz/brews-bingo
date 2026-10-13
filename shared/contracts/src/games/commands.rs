//! Allocation-conscious command receipts; the entire public result is bounded.
use super::*;

pub const COMMAND_RESULT_MAX_BYTES: usize = 4096;
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingCommandOperation {
    Creation,
    LobbyPublication,
    StartPublication,
    TerminalRelease,
    HostTransfer,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingCommandState {
    Prepared,
    AwaitingAcknowledgement,
}
map_enum!(pub CommandResultStatus, "status", {
    Committed { receipt:GameReceipt, follow_up_pending:bool },
    Pending { operation:PendingCommandOperation, state:PendingCommandState },
    Unknown {},
    Expired {},
    Inadmissible {},
});
map_struct!(pub CommandResult {
    #[serde(deserialize_with="version_one")] version:u8,
    game_id:GameId,
    command_id:CommandId,
    result_status:CommandResultStatus
}; validate);
impl CommandResult {
    pub fn validate(&self) -> Result<(), GameDecodeError> {
        if self.version != 1 {
            return Err(GameDecodeError);
        }
        if let CommandResultStatus::Committed { receipt, .. } = &self.result_status {
            receipt.validate()?;
            if receipt.command_id != self.command_id || receipt.game_id != self.game_id {
                return Err(GameDecodeError);
            }
        }
        Ok(())
    }
    pub fn decode_json(bytes: &[u8]) -> Result<Self, GameDecodeError> {
        decode(bytes, COMMAND_RESULT_MAX_BYTES)
    }
    pub fn encode_json(&self) -> Result<Vec<u8>, GameDecodeError> {
        let bytes = encode(self, COMMAND_RESULT_MAX_BYTES)?;
        Self::decode_json(&bytes)?;
        Ok(bytes)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "Closed receipt boundary fixtures fail fast."
)]
mod tests {
    use super::*;
    #[test]
    fn command_lookup_all_closed_receipt_variants_and_total_envelope_bounds() {
        let game: GameId = "01890f3e-53b7-7d28-9b05-4f65092d5701".parse().unwrap();
        let command: CommandId = "01890f3e-53b7-7d28-9b05-4f65092d5702".parse().unwrap();
        let player: PlayerId = "01890f3e-53b7-7d28-9b05-4f65092d5703".parse().unwrap();
        let account: AccountId = "01890f3e-53b7-7d28-9b05-4f65092d5704".parse().unwrap();
        let at = 1_700_000_000_000;
        let expiry = at + GAME_RECEIPT_RETENTION_MS;
        let outcomes = [
            GameOutcome::Created {
                view_revision: 0,
                idle_cancel_due_at: expiry,
            },
            GameOutcome::LobbyOpened { view_revision: 1 },
            GameOutcome::Started {
                view_revision: 2,
                started_at: at,
            },
            GameOutcome::PlayerJoined {
                player_id: player,
                view_revision: 0,
                session_expires_at: expiry,
            },
            GameOutcome::PlayerRecovered {
                player_id: player,
                view_revision: 1,
                session_expires_at: expiry,
            },
            GameOutcome::AliasRenamed {
                player_id: player,
                alias: "Lookup".into(),
                session_expires_at: expiry,
                view_revision: 1,
            },
            GameOutcome::PlayerLeft {
                left: true,
                expected_state: GameState::InProgress,
                membership_retained: true,
                fresh_admission_required: false,
                player_id: player,
                session_expires_at: Some(expiry),
                view_revision: 1,
            },
            GameOutcome::RecoveryAnswerChanged {
                recovery_enabled: true,
                view_revision: 1,
            },
            GameOutcome::ConfigurationUpdated {
                view_revision: 1,
                changed: true,
            },
            GameOutcome::Resumed { view_revision: 1 },
            GameOutcome::HostTransferred {
                view_revision: 1,
                designated_host_id: account,
                host_assignment_revision: 1,
            },
            GameOutcome::Exited {},
            GameOutcome::CallAccepted {
                call: CallView {
                    sequence_no: 1,
                    value: "1".into(),
                },
                remaining_count: 3,
                exhausted: false,
                view_revision: 1,
            },
            GameOutcome::Terminalized {
                expected_state: GameState::New,
                state: GameState::Cancelled,
                ended_at: at,
                history_available: false,
                history_expires_at: None,
                winner: None,
                view_revision: 1,
            },
            GameOutcome::ParticipantChanged {
                change: ParticipantChange::Reconnected,
                identity: ParticipantIdentity::Player {
                    player_id: player,
                    alias: "Lookup".into(),
                },
                session_expires_at: Some(expiry),
                view_revision: 1,
            },
        ];
        for outcome in outcomes {
            let receipt = GameReceipt {
                version: 1,
                command_id: command,
                game_id: game,
                outcome,
                completed_at: at,
                expires_at: expiry,
            };
            let result = CommandResult {
                version: 1,
                game_id: game,
                command_id: command,
                result_status: CommandResultStatus::Committed {
                    receipt,
                    follow_up_pending: true,
                },
            };
            let bytes = result.encode_json().unwrap();
            assert!(bytes.len() <= COMMAND_RESULT_MAX_BYTES);
            assert_eq!(
                CommandResult::decode_json(&bytes)
                    .unwrap()
                    .encode_json()
                    .unwrap(),
                bytes
            );
        }
        for status in [
            CommandResultStatus::Unknown {},
            CommandResultStatus::Expired {},
            CommandResultStatus::Inadmissible {},
            CommandResultStatus::Pending {
                operation: PendingCommandOperation::HostTransfer,
                state: PendingCommandState::Prepared,
            },
        ] {
            let result = CommandResult {
                version: 1,
                game_id: game,
                command_id: command,
                result_status: status,
            };
            let bytes = result.encode_json().unwrap();
            assert!(CommandResult::decode_json(&bytes).is_ok());
            let v = String::from_utf8(bytes).unwrap();
            for raw in [
                v.replace("\"version\":1", "\"version\":2"),
                v.replace("\"version\":1", "\"version\":1,\"version\":1"),
                v.replace("\"version\":1", "\"version\":1,\"private\":true"),
            ] {
                assert!(CommandResult::decode_json(raw.as_bytes()).is_err());
            }
        }
        assert!(CommandResult::decode_json(&vec![b' '; COMMAND_RESULT_MAX_BYTES + 1]).is_err());
    }
}
