"""Independent test-only single-line bingo model. No production imports."""
from copy import deepcopy
import calendar
from datetime import datetime, timedelta, timezone
from .safety import E2EError, require, safe_integer


def call_values(calls: list) -> list[str]:
    require(type(calls) is list and len(calls) <= 75, 'call_ledger_bound')
    values = []
    for sequence, call in enumerate(calls, 1):
        require(type(call) is dict and set(call) == {'sequence_no', 'value'} and
                safe_integer(call['sequence_no'], 1, 75) and call['sequence_no'] == sequence,
                'call_sequence_mismatch')
        value = call['value']
        require(type(value) is str and value in {str(n) for n in range(1, 76)} and
                value not in values, 'call_value_mismatch')
        values.append(value)
    return values


class BoardOracle:
    def __init__(self, board: dict):
        require(type(board) is dict and set(board) == {
            'side_length', 'cells', 'qualified', 'qualifying_lines'} and
            type(board['side_length']) is int and board['side_length'] == 5 and
            type(board['cells']) is list and len(board['cells']) == 25, 'board_shape')
        self.initial = deepcopy(board)
        self.kinds = []
        used = set()
        for index, cell in enumerate(board['cells']):
            require(type(cell) is dict and set(cell) == {'position', 'kind', 'is_matched'}, 'cell_shape')
            position = cell['position']
            require(type(position) is dict and set(position) == {'row', 'column'} and
                    type(position['row']) is int and type(position['column']) is int and
                    position == {'row': index // 5 + 1, 'column': index % 5 + 1}, 'cell_position')
            kind = cell['kind']
            require(type(kind) is dict, 'cell_kind')
            if index == 12:
                require(kind == {'kind': 'free'}, 'free_cell_position')
                self.kinds.append(None)
            else:
                require(set(kind) == {'kind', 'value'} and kind['kind'] == 'value' and
                        type(kind['value']) is str and kind['value'] in {str(n) for n in range(1, 76)} and
                        kind['value'] not in used, 'cell_value')
                used.add(kind['value'])
                self.kinds.append(kind['value'])
        self.verify(board, [])

    def evaluate(self, values: list[str]) -> dict:
        require(type(values) is list and len(values) == len(set(values)) and
                all(type(v) is str and v in {str(n) for n in range(1, 76)} for v in values),
                'oracle_calls_invalid')
        result = deepcopy(self.initial)
        matched = [value is None or value in values for value in self.kinds]
        for cell, yes in zip(result['cells'], matched):
            cell['is_matched'] = yes
        lines = []
        for row in range(5):
            if all(matched[row * 5 + column] for column in range(5)):
                lines.append({'kind': 'row', 'index': row + 1})
        for column in range(5):
            if all(matched[row * 5 + column] for row in range(5)):
                lines.append({'kind': 'column', 'index': column + 1})
        if all(matched[i * 5 + i] for i in range(5)):
            lines.append({'kind': 'main_diagonal'})
        if all(matched[i * 5 + 4 - i] for i in range(5)):
            lines.append({'kind': 'anti_diagonal'})
        result['qualified'], result['qualifying_lines'] = bool(lines), lines
        return result

    def verify(self, board: dict, values: list[str]):
        require(type(board) is dict and board == self.evaluate(values) and
                type(board.get('side_length')) is int and type(board.get('qualified')) is bool and
                type(board.get('qualifying_lines')) is list and all(
                    type(line) is dict and type(line.get('kind')) is str and
                    ('index' not in line or type(line['index']) is int)
                    for line in board['qualifying_lines']) and
                all(type(c['position']['row']) is int and type(c['position']['column']) is int
                    for c in board['cells']) and
                all(type(c.get('is_matched')) is bool for c in board['cells']), 'board_oracle_mismatch')

    def first_row(self):
        return self.kinds[:5]


def history_expiry(ended_at: int) -> int:
    """DO-071: three UTC calendar months, preserving milliseconds and clamping day."""
    require(safe_integer(ended_at, 1), 'history_timestamp_invalid')
    epoch = datetime(1970, 1, 1, tzinfo=timezone.utc)
    try:
        ended = epoch + timedelta(milliseconds=ended_at)
        month_index = ended.year * 12 + ended.month - 1 + 3
        year, month = divmod(month_index, 12)
        month += 1
        expires = ended.replace(year=year, month=month,
                                day=min(ended.day, calendar.monthrange(year, month)[1]))
        delta = expires - epoch
        return (delta.days * 86400 + delta.seconds) * 1000 + delta.microseconds // 1000
    except (ValueError, OverflowError):
        raise E2EError('history_timestamp_invalid') from None
