use serde::{Deserialize, Serialize};

/// GET /api/v1/tools/lookalike?domain=paypa1-secure.com&real=paypal.com
#[derive(Debug, Deserialize)]
pub struct LookalikeQuery {
    /// The website or email address to check, as the visitor typed it.
    #[serde(default)]
    pub domain: String,
    /// The real company's website (optional). Without it, the domain is
    /// compared with famous brands.
    #[serde(default)]
    pub real: String,
}

/// What the Lookalike Domain Checker found. The page shows a matching
/// title, text and color for each one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LookalikeVerdict {
    /// The same website as the real one (or a part of it, like shop.paypal.com).
    Same,
    /// A copy of the real name with a trick (one letter changed, 0 for o...).
    Lookalike,
    /// Close to the real name, but not a clear copy (two letters different,
    /// the same name with another ending). Worth a second look.
    Close,
    /// A different name: it does not look like a copy of the real one.
    Different,
    /// No real website was given and no famous brand is copied.
    NoBrand,
}

/// One trick the domain uses. `a` and `b` are the parts to show, for
/// example a = "1", b = "l" for "1 instead of l".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Trick {
    pub kind: TrickKind,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub a: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub b: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrickKind {
    /// Letters from another alphabet that look like Latin ones (Cyrillic "а").
    ForeignLetters,
    /// A number in place of a letter: "1" for "l", "0" for "o".
    NumberForLetter,
    /// Letters that look like another letter together: "rn" for "m".
    LookalikeLetters,
    /// One letter changed: "x" instead of "y".
    ChangedLetter,
    /// One letter left out.
    MissingLetter,
    /// One letter added.
    ExtraLetter,
    /// Two letters next to each other swapped.
    SwappedLetters,
    /// A dash added inside the name.
    ExtraDash,
    /// The real name with a word added: "paypal-secure".
    ExtraWord,
    /// The same name with another ending: ".co" instead of ".com".
    DifferentEnding,
    /// The real name in front of another website: paypal.com.login-help.xyz.
    NameInFront,
    /// Two small changes.
    TwoChanges,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LookalikeAnswer {
    /// The address that was checked, cleaned: "paypa1-secure.com".
    pub domain: String,
    /// The website it really belongs to: "login-help.xyz" for
    /// "paypal.com.login-help.xyz". The same as `domain` most of the time.
    pub site: String,
    /// The real website it was compared with: what the visitor gave, or
    /// the copied brand's own website.
    pub compared_with: Option<String>,
    /// The famous brand copied, when no real website was given.
    pub brand: Option<String>,
    pub verdict: LookalikeVerdict,
    pub tricks: Vec<Trick>,
}
