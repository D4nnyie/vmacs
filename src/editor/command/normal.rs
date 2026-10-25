use super::{Command, Edit, FoldOp, ModeCommand, Move, Obj, Op, System, Target};
use crossterm::event::{
    Event, KeyCode::{self, Char, End, Home}, KeyEvent, KeyModifiers,
};

const MAX_COUNT: usize = 99_999;

#[derive(Default)]
pub struct NormalState {
    pending_op: Option<char>,
    count: usize,
    pre: usize,
    find: Option<char>,
    after_g: bool,
    object: Option<bool>,
}

fn plain(modifiers: KeyModifiers) -> bool {
    matches!(modifiers, KeyModifiers::NONE | KeyModifiers::SHIFT)
}

fn is_operator(c: char) -> bool {
    matches!(c, 'd' | 'y' | 'c' | '>' | '<' | 'C')
}

fn op_for(c: char) -> Option<Op> {
    Some(match c {
        'd' => Op::Delete,
        'y' => Op::Yank,
        'c' => Op::Change,
        '>' => Op::Indent,
        '<' => Op::Dedent,
        'C' => Op::Comment,
        _ => return None,
    })
}

fn motion_for(c: char) -> Option<Move> {
    Some(match c {
        'h' => Move::Left,
        'l' => Move::Right,
        'j' => Move::Down,
        'k' => Move::Up,
        '0' => Move::StartOfLine,
        '$' => Move::EndOfLine,
        '^' => Move::FirstNonBlank,
        'w' => Move::WordForward,
        'b' => Move::WordBackward,
        'e' => Move::WordEnd,
        'W' => Move::BigWordForward,
        'B' => Move::BigWordBackward,
        'E' => Move::BigWordEnd,
        '{' => Move::ParagraphBackward,
        '}' => Move::ParagraphForward,
        '%' => Move::MatchingBracket,
        'G' => Move::BufferEnd,
        'H' => Move::ScreenTop,
        'M' => Move::ScreenMiddle,
        'L' => Move::ScreenBottom,
        '+' => Move::NextLineStart, 
        '-' => Move::PrevLineStart,
        _ => return None,
    })
}

fn object_for(c: char) -> Option<Obj> {
    Some(match c {
        'w' => Obj::Word,
        'W' => Obj::BigWord,
        'p' => Obj::Paragraph,
        '"' | '\'' | '`' => Obj::Quote(c),
        '(' | ')' | 'b' => Obj::Pair('('),
        '[' | ']' => Obj::Pair('['),
        '{' | '}' | 'B' => Obj::Pair('{'),
        _ => return None,
    })
}

impl Command {
    pub fn try_from_normal(event: Event, state: &mut NormalState) -> Result<Option<Self>, String> {
        let result = Self::normal_key(event, state);
        if result.is_err() {
            state.reset();
        }
        result
    }

    fn operate(op: Op, target: Target, state: &mut NormalState) -> Self {
        Self::Edit(Edit::Operate { op, target, count: state.take_count().unwrap_or(1) })
    }

    fn finish(op: Op, target: Target, state: &mut NormalState) -> Self {
        state.pending_op = None;
        Self::operate(op, target, state)
    }

    fn operator_key(
        opc: char,
        code: KeyCode,
        modifiers: KeyModifiers,
        state: &mut NormalState,
    ) -> Result<Option<Self>, String> {
        let cancel = || "operator cancelled".to_string();
        if code == KeyCode::Esc {
            state.reset();
            return Ok(None);
        }
        let Char(c) = code else {
            return Err(cancel());
        };
        let Some(op) = op_for(opc) else {
            return Err(cancel());
        };
        if !plain(modifiers) {
            return Err(cancel());
        }
        if let Some(kind) = state.find.take() {
            let target = Target::Motion(Move::Find {
                ch: c,
                forward: matches!(kind, 'f' | 't'),
                till: matches!(kind, 't' | 'T'),
            });
            return Ok(Some(Self::finish(op, target, state)));
        }
        if let Some(around) = state.object.take() {
            let Some(obj) = object_for(c) else {
                return Err(cancel());
            };
            return Ok(Some(Self::finish(op, Target::Object { obj, around }, state)));
        }
        if state.after_g {
            state.after_g = false;
            let motion = match c {
                'g' => Move::BufferStart,
                '_' => Move::LastNonBlank,
                _ => return Err(cancel()),
            };
            return Ok(Some(Self::finish(op, Target::Motion(motion), state)));
        }
        if let Some(n) = c.to_digit(10).and_then(|n| usize::try_from(n).ok()) {
            if n != 0 || state.count > 0 {
                state.count = state.count.saturating_mul(10).saturating_add(n).min(MAX_COUNT);
                return Ok(None);
            }
        }
        // `cc` keeps its old meaning: copy to the system clipboard.
        if opc == 'c' && c == 'c' {
            state.reset();
            return Ok(Some(Self::Edit(Edit::YankSelectionSystem)));
        }
        if c == opc || (opc == 'C' && c == 'c') {
            return Ok(Some(Self::finish(op, Target::Line, state)));
        }
        match c {
            'i' | 'a' => {
                state.object = Some(c == 'a');
                Ok(None)
            }
            'f' | 'F' | 't' | 'T' => {
                state.find = Some(c);
                Ok(None)
            }
            'g' => {
                state.after_g = true;
                Ok(None)
            }
            _ => match motion_for(c) {
                Some(motion) => Ok(Some(Self::finish(op, Target::Motion(motion), state))),
                None => Err(cancel()),
            },
        }
    }

    fn prefix_key(
        prefix: char,
        code: KeyCode,
        modifiers: KeyModifiers,
        state: &mut NormalState,
    ) -> Result<Option<Self>, String> {
        let unsupported = || format!("Unsupported {prefix}+{code:?}");
        let Char(c) = code else {
            return Err(unsupported());
        };
        if !plain(modifiers) {
            return Err(unsupported());
        }
        let fold = |op| Ok(Some(Self::Mode(ModeCommand::Fold(op))));
        match (prefix, c) {
            ('g', 'g') => Ok(Some(Self::Move(Move::BufferStart))),
            ('g', '_') => Ok(Some(Self::Move(Move::LastNonBlank))),
            ('g', 'c') => {
                state.set_op('C');
                Ok(None)
            }
            ('z', 't') => Ok(Some(Self::Move(Move::ViewTop))),
            ('z', 'z') => Ok(Some(Self::Move(Move::ViewCenter))),
            ('z', 'b') => Ok(Some(Self::Move(Move::ViewBottom))),
            ('z', 'a') => fold(FoldOp::Toggle),
            ('z', 'o') => fold(FoldOp::Open),
            ('z', 'c') => fold(FoldOp::Close),
            ('z', 'R') => fold(FoldOp::OpenAll),
            ('z', 'M') => fold(FoldOp::CloseAll),
            ('z', 'j') => fold(FoldOp::Next),
            ('z', 'k') => fold(FoldOp::Prev),
            ('z', 'h') => Ok(Some(Self::Move(Move::ViewLeft))),
            ('z', 'l') => Ok(Some(Self::Move(Move::ViewRight))),
            ('z', 'H') => Ok(Some(Self::Move(Move::ViewHalfLeft))),
            ('z', 'L') => Ok(Some(Self::Move(Move::ViewHalfRight))),
            ('z', 's') => Ok(Some(Self::Move(Move::ViewColStart))),
            ('z', 'e') => Ok(Some(Self::Move(Move::ViewColEnd))),
            ('r', 'r') => Ok(Some(Self::Edit(Edit::Redo))),
            ('r', ch) => {
                let n = state.take_count().unwrap_or(1);
                Ok(Some(Self::Edit(Edit::ReplaceChar(ch, n))))
            }
            ('f' | 'F' | 't' | 'T', ch) => Ok(Some(Self::Move(Move::Find {
                ch,
                forward: matches!(prefix, 'f' | 't'),
                till: matches!(prefix, 't' | 'T'),
            }))),
            _ => Err(unsupported()),
        }
    }

    fn normal_key(event: Event, state: &mut NormalState) -> Result<Option<Self>, String> {
        let Event::Key(key_event) = event else {
            return Command::try_from(event).map(Some);
        };
        let KeyEvent { code, modifiers, .. } = key_event;

        if let Some(pending) = state.pending_op {
            if is_operator(pending) {
                return Self::operator_key(pending, code, modifiers, state);
            }
            state.pending_op = None;
            return Self::prefix_key(pending, code, modifiers, state);
        }

        if plain(modifiers) {
            if let Char(digit) = code {
                if let Some(n) = digit.to_digit(10).and_then(|n| usize::try_from(n).ok()) {
                    if n != 0 || state.count > 0 {
                        state.count =
                            state.count.saturating_mul(10).saturating_add(n).min(MAX_COUNT);
                        return Ok(None);
                    }
                }
            }
            match code {
                Char(c @ ('d' | 'y' | 'c' | '>' | '<')) => {
                    state.set_op(c);
                    return Ok(None);
                }
                Char(c @ ('g' | 'z' | 'r' | 'f' | 'F' | 't' | 'T')) => {
                    state.pending_op = Some(c);
                    return Ok(None);
                }
                Char('Y') => return Ok(Some(Self::operate(Op::Yank, Target::Line, state))),
                Char('C') => {
                    return Ok(Some(Self::operate(Op::Change, Target::Motion(Move::EndOfLine), state)));
                }
                Char('S') => return Ok(Some(Self::operate(Op::Change, Target::Line, state))),
                Char('s') => {
                    return Ok(Some(Self::operate(Op::Change, Target::Motion(Move::Right), state)));
                }
                Char('X') => {
                    return Ok(Some(Self::operate(Op::Delete, Target::Motion(Move::Left), state)));
                }
                Char('J') => {
                    return Ok(Some(Self::Edit(Edit::JoinLines(state.take_count().unwrap_or(1)))));
                }
                Char('.') => return Ok(Some(Self::Edit(Edit::Repeat))),
                Char('~') => {
                    return Ok(Some(Self::Edit(Edit::ToggleCase(state.take_count().unwrap_or(1)))));
                }
                Char('h') => return Ok(Some(Self::Move(Move::Left))),
                Char('l') => return Ok(Some(Self::Move(Move::Right))),
                Char('k') => return Ok(Some(Self::Move(Move::Up))),
                Char('j') => return Ok(Some(Self::Move(Move::Down))),
                Char('0') => return Ok(Some(Self::Move(Move::StartOfLine))),
                Char('$') => return Ok(Some(Self::Move(Move::EndOfLine))),
                Char('w') => return Ok(Some(Self::Move(Move::WordForward))),
                Char('b') => return Ok(Some(Self::Move(Move::WordBackward))),
                Char('e') => return Ok(Some(Self::Move(Move::WordEnd))),
                Char('W') => return Ok(Some(Self::Move(Move::BigWordForward))),
                Char('B') => return Ok(Some(Self::Move(Move::BigWordBackward))),
                Char('E') => return Ok(Some(Self::Move(Move::BigWordEnd))),
                Char('H') => return Ok(Some(Self::Move(Move::ScreenTop))),
                Char('M') => return Ok(Some(Self::Move(Move::ScreenMiddle))),
                Char('L') => return Ok(Some(Self::Move(Move::ScreenBottom))),
                Char('^') => return Ok(Some(Self::Move(Move::FirstNonBlank))),
                Char('%') => return Ok(Some(Self::Move(Move::MatchingBracket))),
                Char('{') => return Ok(Some(Self::Move(Move::ParagraphBackward))),
                Char('}') => return Ok(Some(Self::Move(Move::ParagraphForward))),
                Char('G') => return Ok(Some(Self::Move(Move::BufferEnd))),
                Char('x') => return Ok(Some(Self::Edit(Edit::DeleteCharYank))),
                Char('D') => return Ok(Some(Self::Edit(Edit::DeleteToLineEnd))),
                Char('p') => return Ok(Some(Self::Edit(Edit::Paste))),
                Char('P') => return Ok(Some(Self::Edit(Edit::PasteBefore))),
                Char('i') => return Ok(Some(Self::Mode(ModeCommand::EnterInsert))),
                Char('a') => return Ok(Some(Self::Mode(ModeCommand::EnterInsertAfter))),
                Char('I') => return Ok(Some(Self::Mode(ModeCommand::EnterInsertLineStart))),
                Char('A') => return Ok(Some(Self::Mode(ModeCommand::EnterInsertLineEnd))),
                Char('o') => return Ok(Some(Self::Mode(ModeCommand::EnterInsertNewlineBelow))),
                Char('O') => return Ok(Some(Self::Mode(ModeCommand::EnterInsertNewlineAbove))),
                Char(':') => return Ok(Some(Self::System(System::CommandPrompt))),
                Char('u') => return Ok(Some(Self::Edit(Edit::Undo))),
                Char('v') => return Ok(Some(Self::Mode(ModeCommand::EnterVisual))),
                Char('+') => return Ok(Some(Self::Move(Move::NextLineStart))),
                Char('-') => return Ok(Some(Self::Move(Move::PrevLineStart))),
                KeyCode::Tab => return Ok(Some(Self::Move(Move::JumpForward))),
                KeyCode::Esc => return Ok(Some(Self::System(System::Dismiss))),
                _ => {}
            }
        }

        // Emacs bleed-through, even in Control mode
        if modifiers == KeyModifiers::CONTROL {
            match code {
                Char('s') => return Ok(Some(Self::System(System::Search))),
                Char('x') => return Ok(Some(Self::System(System::Save))),
                Char('w') => return Ok(Some(Self::System(System::SaveAs))),
                Char('q') => return Ok(Some(Self::System(System::Quit))),
                Char('d') => return Ok(Some(Self::Move(Move::HalfPageDown))),
                Char('u') => return Ok(Some(Self::Move(Move::HalfPageUp))),
                Char('o') => return Ok(Some(Self::Move(Move::JumpBack))),
                Char('k') => return Ok(Some(Self::Edit(Edit::DeleteToLineEnd))),
                Char('y') => return Ok(Some(Self::Edit(Edit::PasteInline))),
                Char('n') => return Ok(Some(Self::Move(Move::Down))),
                Char('p') => return Ok(Some(Self::Move(Move::Up))),
                Char('f') => return Ok(Some(Self::Move(Move::Right))),
                Char('b') => return Ok(Some(Self::Move(Move::Left))),
                Char('a') => return Ok(Some(Self::Move(Move::StartOfLine))),
                Char('e') => return Ok(Some(Self::Move(Move::EndOfLine))),
                Char('h') => return Ok(Some(Self::Edit(Edit::DeleteBackward))),
                Char('r') => return Ok(Some(Self::Edit(Edit::Redo))),
                Char('z') => return Ok(Some(Self::Edit(Edit::Undo))),
                Home => return Ok(Some(Self::Move(Move::BufferStart))),
                End => return Ok(Some(Self::Move(Move::BufferEnd))),
                _ => {}
            }
        }

        if matches!(modifiers, KeyModifiers::META | KeyModifiers::ALT) {
            match code {
                Char('f') => return Ok(Some(Self::Move(Move::WordForward))),
                Char('b') => return Ok(Some(Self::Move(Move::WordBackward))),
                Char('w') => return Ok(Some(Self::Edit(Edit::YankSelection))),
                Char('<') => return Ok(Some(Self::Move(Move::BufferStart))),
                Char('>') => return Ok(Some(Self::Move(Move::BufferEnd))),
                Char('c') => return Ok(Some(Self::Edit(Edit::YankSelectionSystem))),
                Char('x') => return Ok(Some(Self::System(System::CommandPrompt))),
                _ => {}
            }
        }

        if let Ok(motion) = Move::try_from_shifted(key_event) {
            return Ok(Some(Self::Select(motion)));
        }
        if let Ok(mv) = Move::try_from(key_event) {
            return Ok(Some(Self::Move(mv)));
        }
        if let Ok(sys) = System::try_from(key_event) {
            return Ok(Some(Self::System(sys)));
        }

        Err(format!("Unsupported key in Normal mode: {code:?}"))
    }
}

impl NormalState {
    pub const fn is_pending(&self) -> bool {
        self.pending_op.is_some() || self.find.is_some() || self.after_g || self.object.is_some()
    }

    fn set_op(&mut self, op: char) {
        self.pre = std::mem::take(&mut self.count);
        self.pending_op = Some(op);
    }

    fn reset(&mut self) {
        *self = Self::default();
    }

    #[allow(dead_code)]
    pub fn clear_count(&mut self) {
        self.count = 0;
        self.pre = 0;
    }

    pub fn take_count(&mut self) -> Option<usize> {
        let (pre, cur) = (std::mem::take(&mut self.pre), std::mem::take(&mut self.count));
        (pre != 0 || cur != 0).then(|| pre.max(1).saturating_mul(cur.max(1)).min(MAX_COUNT))
    }

    pub fn begin_find(&mut self, op: char) {
        self.pending_op = Some(op);
    }

    pub fn take_find(&mut self) -> Option<char> {
        match self.pending_op {
            Some(op @ ('f' | 'F' | 't' | 'T')) => {
                self.pending_op = None;
                Some(op)
            }
            _ => None,
        }
    }
}