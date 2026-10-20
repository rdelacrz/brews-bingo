use crate::components::Notice;
use brews_domain::games::{
    BOARD_SIDE_MAX, BOARD_SIDE_MIN, CellPosition, GameConfiguration, WinningPattern,
};
use dioxus::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
enum ConfigurationError {
    #[error("Enter whole numbers within the displayed limits.")]
    Integer,
    #[error(
        "Check the pool, board size, capacities, and unique free-cell coordinates. The pool must cover every non-free cell."
    )]
    Invalid,
}
fn configuration(
    pool: &str,
    side: &str,
    players: &str,
    spectators: &str,
    enabled: bool,
    positions: Vec<CellPosition>,
) -> Result<GameConfiguration, ConfigurationError> {
    fn number<T: std::str::FromStr>(text: &str) -> Result<T, ConfigurationError> {
        if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
            return Err(ConfigurationError::Integer);
        }
        text.parse().map_err(|_| ConfigurationError::Integer)
    }
    let value = GameConfiguration {
        numeric_upper_bound: number(pool)?,
        board_side_length: number(side)?,
        player_capacity: number(players)?,
        spectator_capacity: number(spectators)?,
        free_cells_enabled: enabled,
        free_cell_positions: if enabled { positions } else { Vec::new() },
        winning_pattern: WinningPattern::SingleLine,
    };
    value.validate().map_err(|_| ConfigurationError::Invalid)?;
    Ok(value)
}

#[component]
pub(super) fn ConfigurationEditor(
    initial: GameConfiguration,
    busy: bool,
    label: String,
    on_save: EventHandler<GameConfiguration>,
    on_dirty: Option<EventHandler<bool>>,
) -> Element {
    let mut pool = use_signal(|| initial.numeric_upper_bound.to_string());
    let mut side = use_signal(|| initial.board_side_length.to_string());
    let mut players = use_signal(|| initial.player_capacity.to_string());
    let mut spectators = use_signal(|| initial.spectator_capacity.to_string());
    let mut enabled = use_signal(|| initial.free_cells_enabled);
    let mut positions = use_signal(|| initial.free_cell_positions.clone());
    let mut error = use_signal(|| None::<String>);
    use_effect(use_reactive((&initial,), move |(initial,)| {
        let changed = configuration(
            &pool(),
            &side(),
            &players(),
            &spectators(),
            enabled(),
            positions(),
        )
        .map_or(true, |draft| draft != initial);
        if let Some(on_dirty) = on_dirty {
            on_dirty.call(changed);
        }
    }));
    let size = side()
        .parse::<u8>()
        .ok()
        .filter(|n| (BOARD_SIDE_MIN..=BOARD_SIDE_MAX).contains(n));
    let validated = configuration(
        &pool(),
        &side(),
        &players(),
        &spectators(),
        enabled(),
        positions(),
    );
    let infeasible = validated
        .as_ref()
        .is_ok_and(|c| c.validate_start_feasibility(2).is_err());
    let position_summary = positions
        .read()
        .iter()
        .map(|p| format!("({}, {})", p.row, p.column))
        .collect::<Vec<_>>()
        .join(", ");
    rsx! {form {class:"form card stack",onsubmit:move |event| {
        event.prevent_default();
        match configuration(&pool(),&side(),&players(),&spectators(),enabled(),positions()) {Ok(value) if !busy=>{error.set(None);on_save.call(value);},Err(reason)=>error.set(Some(reason.to_string())),_=>{}}
    },
        label {class:"field","Numeric pool upper bound (1–1,000)" input {r#type:"text",inputmode:"numeric",value:pool,disabled:busy,oninput:move |e|pool.set(e.value())}}
        label {class:"field","Board side length (2–10)" input {r#type:"text",inputmode:"numeric",value:side,disabled:busy,oninput:move |e|side.set(e.value())}}
        label {class:"field","Player capacity (2–20)" input {r#type:"text",inputmode:"numeric",value:players,disabled:busy,oninput:move |e|players.set(e.value())}}
        label {class:"field","Spectator capacity (0–50)" input {r#type:"text",inputmode:"numeric",value:spectators,disabled:busy,oninput:move |e|spectators.set(e.value())}}
        label {class:"field",input {r#type:"checkbox",checked:enabled,disabled:busy,onchange:move |e|enabled.set(e.checked())}"Enable free cells"}
        if enabled() {
            p {"Choose free cells by coordinate. Changing size preserves your selection until you correct or reset it."}
            if let Some(size)=size {
                div {class:"board",style:"grid-template-columns: repeat({size}, minmax(2.5rem, 1fr));",role:"group","aria-label":"Free-cell setup",
                    for row in 1..=size {for column in 1..=size {
                        {let position=CellPosition{row,column};let selected=positions.read().contains(&position);rsx!{
                            button {r#type:"button",class:if selected {"board-cell free"}else{"board-cell"},disabled:busy,"aria-pressed":selected,"aria-label":format!("Row {row}, column {column}, {}",if selected{"free"}else{"value"}),onclick:move |_|positions.with_mut(|values|{if let Some(index)=values.iter().position(|p|*p==position){values.remove(index);}else{values.push(position);values.sort();}}),"{row},{column}"}
                        }}
                    }}
                }
                button {r#type:"button",class:"button secondary",disabled:busy,onclick:move |_|positions.set(vec![CellPosition{row:size.div_ceil(2),column:size.div_ceil(2)}]),"Reset free cells to center"}
            }
            p {class:"muted","Selected positions: {position_summary}"}
        }
        p {"Winning pattern: Single Line — one complete row, column, or diagonal."}
        if infeasible {Notice {message:"This layout cannot supply distinct boards for two players. Correct it before Start."}}
        if let Some(message)=error() {Notice {message}}
        button {class:"button",r#type:"submit",disabled:busy,"{label}"}
    }}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_invalid_shrink_and_small_pool_without_dropping_positions() {
        let positions = vec![CellPosition { row: 5, column: 5 }];
        assert!(configuration("75", "2", "20", "50", true, positions).is_err());
        assert!(configuration("3", "5", "20", "50", false, Vec::new()).is_err());
        assert!(configuration("75", "5", "20", "50", false, Vec::new()).is_ok());
        assert!(configuration("7.5", "5", "20", "50", false, Vec::new()).is_err());
    }
}
