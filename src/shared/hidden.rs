//! Hidden-formatted text: Word's hidden character property (DOCX
//! `w:vanish`, RTF `\v`, binary DOC `fVanish`). Frontends leave the text
//! out of the document model and count what they left out
//! (`Document::hidden_text_left_out`).

/// Counts the stretches of hidden text a frontend leaves out.
///
/// The unit is a *stretch*: hidden text running on without visible text in
/// between, within one paragraph, counts once however many runs or format
/// changes it spans. A stretch counts only once it holds a character that
/// is not whitespace, so hidden spaces or tabs alone count nothing.
#[derive(Debug, Default)]
pub struct HiddenText {
    count: u32,
    in_stretch: bool,
}

impl HiddenText {
    /// Hidden text was left out.
    pub fn left_out(&mut self, text: &str) {
        if self.in_stretch || text.chars().all(char::is_whitespace) {
            return;
        }
        self.in_stretch = true;
        self.count = self.count.saturating_add(1);
    }

    /// Visible text, or a paragraph boundary: the next hidden text starts a
    /// new stretch.
    pub fn visible(&mut self) {
        self.in_stretch = false;
    }

    /// The stretches left out so far.
    pub fn count(&self) -> u32 {
        self.count
    }
}

#[cfg(test)]
mod tests {
    use super::HiddenText;

    #[test]
    fn a_stretch_counts_once_however_many_runs_it_spans() {
        let mut hidden = HiddenText::default();
        hidden.left_out("secret ");
        hidden.left_out("words");
        assert_eq!(hidden.count(), 1);
        hidden.visible();
        hidden.left_out("again");
        assert_eq!(hidden.count(), 2);
    }

    #[test]
    fn whitespace_alone_counts_nothing() {
        let mut hidden = HiddenText::default();
        hidden.left_out(" \t");
        assert_eq!(hidden.count(), 0);
        // The stretch starts with its first visible-looking character.
        hidden.left_out("x");
        hidden.left_out("y");
        assert_eq!(hidden.count(), 1);
    }
}
