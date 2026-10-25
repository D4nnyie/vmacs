use super::{Command, Edit, FoldOp, ModeCommand, Move, Op, System, Target};

pub struct Action {
    pub name: &'static str,
    pub desc: &'static str,
    pub command: Command,
}

const fn action(name: &'static str, desc: &'static str, command: Command) -> Action {
    Action { name, desc, command }
}

pub const ACTIONS: &[Action] = &[
    action("move-left", "cursor left", Command::Move(Move::Left)),
    action("move-right", "cursor right", Command::Move(Move::Right)),
    action("move-up", "cursor up", Command::Move(Move::Up)),
    action("move-down", "cursor down", Command::Move(Move::Down)),
    action("line-start", "start of line", Command::Move(Move::StartOfLine)),
    action("line-end", "end of line", Command::Move(Move::EndOfLine)),
    action("word-forward", "next word", Command::Move(Move::WordForward)),
    action("word-backward", "previous word", Command::Move(Move::WordBackward)),
    action("paragraph-forward", "next paragraph (blank line)", Command::Move(Move::ParagraphForward)),
    action("paragraph-backward", "previous paragraph (blank line)", Command::Move(Move::ParagraphBackward)),
    action("page-up", "page up", Command::Move(Move::PageUp)),
    action("page-down", "page down", Command::Move(Move::PageDown)),
    action("buffer-start", "start of buffer", Command::Move(Move::BufferStart)),
    action("buffer-end", "end of buffer", Command::Move(Move::BufferEnd)),
    action("select-left", "extend selection left", Command::Select(Move::Left)),
    action("select-right", "extend selection right", Command::Select(Move::Right)),
    action("select-up", "extend selection up", Command::Select(Move::Up)),
    action("select-down", "extend selection down", Command::Select(Move::Down)),
    action("select-line-start", "extend to line start", Command::Select(Move::StartOfLine)),
    action("select-line-end", "extend to line end", Command::Select(Move::EndOfLine)),
    action("select-word-forward", "extend by word", Command::Select(Move::WordForward)),
    action("select-word-backward", "extend back by word", Command::Select(Move::WordBackward)),
    action("select-paragraph-forward", "extend to next paragraph", Command::Select(Move::ParagraphForward)),
    action("select-paragraph-backward", "extend to previous paragraph", Command::Select(Move::ParagraphBackward)),
    action("select-page-up", "extend a page up", Command::Select(Move::PageUp)),
    action("select-page-down", "extend a page down", Command::Select(Move::PageDown)),
    action("select-buffer-start", "extend to buffer start", Command::Select(Move::BufferStart)),
    action("select-buffer-end", "extend to buffer end", Command::Select(Move::BufferEnd)),
    action("delete", "delete under the cursor (joins lines at the end)", Command::Edit(Edit::Delete)),
    action("delete-backward", "delete before the cursor", Command::Edit(Edit::DeleteBackward)),
    action("delete-char", "delete the character and yank it", Command::Edit(Edit::DeleteCharYank)),
    action("delete-line", "delete the line (yanks it)", Command::Edit(Edit::DeleteLine)),
    action("delete-to-line-end", "delete to the end of the line", Command::Edit(Edit::DeleteToLineEnd)),
    action("yank-line", "copy the line", Command::Edit(Edit::YankLine)),
    action("paste", "paste after the cursor", Command::Edit(Edit::Paste)),
    action("paste-before", "paste before the cursor", Command::Edit(Edit::PasteBefore)),
    action("paste-inline", "paste the register at the cursor", Command::Edit(Edit::PasteInline)),
    action("delete-selection", "delete the selection", Command::Edit(Edit::DeleteSelection)),
    action("yank-selection", "copy the selection", Command::Edit(Edit::YankSelection)),
    action("copy-selection", "copy the selection to the system clipboard", Command::Edit(Edit::YankSelectionSystem)),
    action("uppercase-selection", "UPPERCASE the selection", Command::Edit(Edit::UppercaseSelection)),
    action("lowercase-selection", "lowercase the selection", Command::Edit(Edit::LowercaseSelection)),
    action("indent-selection", "indent the selected lines", Command::Edit(Edit::IndentSelection)),
    action("dedent-selection", "dedent the selected lines", Command::Edit(Edit::DedentSelection)),
    action("indent", "indent to the next stop / indent selection", Command::Edit(Edit::InsertIndent)),
    action("newline", "insert a newline (autoindent)", Command::Edit(Edit::InsertNewline)),
    action("undo", "undo", Command::Edit(Edit::Undo)),
    action("redo", "redo", Command::Edit(Edit::Redo)),
    action("save", "save", Command::System(System::Save)),
    action("save-as", "save under a new name", Command::System(System::SaveAs)),
    action("quit", "quit (unsaved changes: repeat 3 times)", Command::System(System::Quit)),
    action("search", "incremental search", Command::System(System::Search)),
    action("command-prompt", "open the : prompt", Command::System(System::CommandPrompt)),
    action("control-mode", "go to Control mode", Command::Mode(ModeCommand::EnterNormal)),
    action("input-mode", "Input mode before the cursor", Command::Mode(ModeCommand::EnterInsert)),
    action("input-after", "Input mode after the cursor", Command::Mode(ModeCommand::EnterInsertAfter)),
    action("input-line-start", "Input mode at line start", Command::Mode(ModeCommand::EnterInsertLineStart)),
    action("input-line-end", "Input mode at line end", Command::Mode(ModeCommand::EnterInsertLineEnd)),
    action("open-below", "open a line below", Command::Mode(ModeCommand::EnterInsertNewlineBelow)),
    action("open-above", "open a line above", Command::Mode(ModeCommand::EnterInsertNewlineAbove)),
    action("mark-mode", "enter Mark mode", Command::Mode(ModeCommand::EnterVisual)),
    action("word-end", "end of word", Command::Move(Move::WordEnd)),
    action("first-non-blank", "first non-blank character", Command::Move(Move::FirstNonBlank)),
    action("last-non-blank", "last non-blank character", Command::Move(Move::LastNonBlank)),
    action("matching-bracket", "jump to the matching bracket", Command::Move(Move::MatchingBracket)),
    action("half-page-up", "half a screen up", Command::Move(Move::HalfPageUp)),
    action("half-page-down", "half a screen down", Command::Move(Move::HalfPageDown)),
    action("select-word-end", "extend to end of word", Command::Select(Move::WordEnd)),
    action("select-matching-bracket", "extend to the matching bracket", Command::Select(Move::MatchingBracket)),
    action("smart-home", "first non-blank, press again for column 0", Command::Move(Move::SmartHome)),
    action("screen-top", "top visible line", Command::Move(Move::ScreenTop)),
    action("screen-middle", "middle visible line", Command::Move(Move::ScreenMiddle)),
    action("screen-bottom", "bottom visible line", Command::Move(Move::ScreenBottom)),
    action("view-up", "scroll the view up a line (cursor stays)", Command::Move(Move::ViewUp)),
    action("view-down", "scroll the view down a line (cursor stays)", Command::Move(Move::ViewDown)),
    action("view-top", "scroll so the cursor line is at the top", Command::Move(Move::ViewTop)),
    action("view-center", "scroll so the cursor line is centered", Command::Move(Move::ViewCenter)),
    action("view-bottom", "scroll so the cursor line is at the bottom", Command::Move(Move::ViewBottom)),
    action("select-smart-home", "extend to first non-blank / column 0", Command::Select(Move::SmartHome)),
    action("jump-back", "back in the jump list", Command::Move(Move::JumpBack)),
    action("jump-forward", "forward in the jump list", Command::Move(Move::JumpForward)),
    action("fold-toggle", "toggle the fold at the cursor", Command::Mode(ModeCommand::Fold(FoldOp::Toggle))),
    action("fold-open", "open the fold at the cursor", Command::Mode(ModeCommand::Fold(FoldOp::Open))),
    action("fold-close", "close the fold at the cursor", Command::Mode(ModeCommand::Fold(FoldOp::Close))),
    action("fold-open-all", "open every fold", Command::Mode(ModeCommand::Fold(FoldOp::OpenAll))),
    action("fold-close-all", "close every fold", Command::Mode(ModeCommand::Fold(FoldOp::CloseAll))),
    action("fold-next", "go to the next fold", Command::Mode(ModeCommand::Fold(FoldOp::Next))),
    action("fold-prev", "go to the end of the previous fold", Command::Mode(ModeCommand::Fold(FoldOp::Prev))),
    action("bigword-forward", "next WORD", Command::Move(Move::BigWordForward)),
    action("bigword-backward", "previous WORD", Command::Move(Move::BigWordBackward)),
    action("bigword-end", "end of WORD", Command::Move(Move::BigWordEnd)),
    action("join-lines", "join this line with the next", Command::Edit(Edit::JoinLines(1))),
    action("toggle-case", "toggle case under the cursor", Command::Edit(Edit::ToggleCase(1))),
    action("comment-line", "toggle comment on the line", Command::Edit(Edit::Operate { op: Op::Comment, target: Target::Line, count: 1 })),
    action("scroll-left", "scroll the view left", Command::Move(Move::ViewLeft)),
    action("scroll-right", "scroll the view right", Command::Move(Move::ViewRight)),
    action("scroll-half-left", "scroll half a screen left", Command::Move(Move::ViewHalfLeft)),
    action("scroll-half-right", "scroll half a screen right", Command::Move(Move::ViewHalfRight)),
    action("scroll-col-start", "cursor column to the left edge", Command::Move(Move::ViewColStart)),
    action("scroll-col-end", "cursor column to the right edge", Command::Move(Move::ViewColEnd)),
    action("next-line-start", "first non-blank of the next line", Command::Move(Move::NextLineStart)),
    action("prev-line-start", "first non-blank of the previous line", Command::Move(Move::PrevLineStart)),
];

pub fn find_action(name: &str) -> Option<Command> {
    ACTIONS.iter().find(|a| a.name == name).map(|a| a.command)
}

pub const CATEGORIES: &[&str] =
    &["Navigation", "Selection", "Editing", "Modes", "Folding", "Files & commands"];

pub fn category(name: &str) -> &'static str {
    if name.starts_with("select-") || name.ends_with("-selection") {
        "Selection"
    } else if name.starts_with("fold-") {
        "Folding"
    } else if matches!(name, "save" | "save-as" | "quit" | "search" | "command-prompt") {
        "Files & commands"
    } else if name.starts_with("input-") || name.starts_with("open-") || name.ends_with("-mode") {
        "Modes"
    } else if name.starts_with("delete")
        || name.starts_with("yank")
        || name.starts_with("paste")
        || matches!(name, "indent" | "newline" | "undo" | "redo" | "join-lines" | "toggle-case" | "comment-line")
    {
        "Editing"
    } else {
        "Navigation"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn names_are_unique_and_findable() {
        let mut seen = HashSet::new();
        for a in ACTIONS {
            assert!(seen.insert(a.name), "duplicate action {}", a.name);
            assert!(find_action(a.name).is_some());
        }
        assert!(find_action("nope").is_none());
    }
}