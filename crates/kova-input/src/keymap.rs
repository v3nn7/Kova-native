//! Actions and key bindings.
//!
//! An [`Action`] is any `Clone + Debug + 'static` type. Key bindings map
//! keystroke sequences (e.g. `"ctrl-k ctrl-s"`) to actions, optionally only
//! inside a named key context (e.g. `"TextInput"`). The UI dispatches matched
//! actions along the focus path, so the innermost focused element that
//! handles an action wins.

use crate::{Keystroke, KeystrokeParseError};
use std::any::{Any, TypeId};
use std::fmt::Debug;

/// A command that can be triggered by key bindings or programmatically.
///
/// Implemented automatically for every `Clone + Debug + 'static` type:
///
/// ```
/// #[derive(Clone, Debug)]
/// struct Save;
/// let action: Box<dyn kova_input::Action> = Box::new(Save);
/// assert!(action.name().ends_with("Save"));
/// ```
pub trait Action: Any + Debug {
    fn boxed_clone(&self) -> Box<dyn Action>;
    fn name(&self) -> &'static str;
    fn as_any(&self) -> &dyn Any;
    fn action_type(&self) -> TypeId;
}

impl<T: Any + Clone + Debug> Action for T {
    fn boxed_clone(&self) -> Box<dyn Action> {
        Box::new(self.clone())
    }

    fn name(&self) -> &'static str {
        std::any::type_name::<T>()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn action_type(&self) -> TypeId {
        TypeId::of::<T>()
    }
}

// Note: `Box<dyn Action>` deliberately does *not* implement `Clone`; if it
// did, the blanket impl above would make the box itself an `Action` and
// method calls like `boxed.action_type()` would report the box's type.

/// Declares unit-struct actions: `actions!(Save, Quit, ToggleSidebar);`
#[macro_export]
macro_rules! actions {
    ($($name:ident),* $(,)?) => {
        $(
            #[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
            pub struct $name;
        )*
    };
}

/// A binding from a keystroke sequence to an action.
#[derive(Debug)]
pub struct KeyBinding {
    pub keystrokes: Vec<Keystroke>,
    pub action: Box<dyn Action>,
    /// If set, the binding is only active when an element on the focus path
    /// declares this key context.
    pub context: Option<&'static str>,
}

impl Clone for KeyBinding {
    fn clone(&self) -> Self {
        KeyBinding {
            keystrokes: self.keystrokes.clone(),
            action: self.action.boxed_clone(),
            context: self.context,
        }
    }
}

impl KeyBinding {
    /// Creates a binding. Panics on an invalid keystroke string (bindings are
    /// almost always literals; use [`KeyBinding::try_new`] for user input).
    pub fn new(keystrokes: &str, action: impl Action) -> KeyBinding {
        Self::try_new(keystrokes, action).unwrap_or_else(|e| panic!("{e}"))
    }

    pub fn try_new(
        keystrokes: &str,
        action: impl Action,
    ) -> Result<KeyBinding, KeystrokeParseError> {
        let keystrokes = keystrokes
            .split_whitespace()
            .map(Keystroke::parse)
            .collect::<Result<Vec<_>, _>>()?;
        if keystrokes.is_empty() {
            return Err(KeystrokeParseError(String::new()));
        }
        Ok(KeyBinding {
            keystrokes,
            action: Box::new(action),
            context: None,
        })
    }

    pub fn context(mut self, context: &'static str) -> Self {
        self.context = Some(context);
        self
    }
}

/// Result of feeding a keystroke into the [`Keymap`].
#[derive(Debug)]
pub enum KeymapMatch {
    /// No binding matches; the keystroke should be handled as raw input.
    None,
    /// A prefix of a multi-keystroke binding was typed; wait for more.
    Pending,
    /// Bindings matched, most specific (context-bound, later-defined) first.
    Matched(Vec<Box<dyn Action>>),
}

/// A collection of key bindings plus multi-keystroke matching state.
#[derive(Default)]
pub struct Keymap {
    bindings: Vec<KeyBinding>,
    pending: Vec<Keystroke>,
}

impl Keymap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, binding: KeyBinding) {
        self.bindings.push(binding);
    }

    pub fn extend(&mut self, bindings: impl IntoIterator<Item = KeyBinding>) {
        self.bindings.extend(bindings);
    }

    pub fn bindings(&self) -> &[KeyBinding] {
        &self.bindings
    }

    /// All bindings for a given action type (for showing shortcuts in menus).
    pub fn bindings_for(&self, action: &dyn Action) -> impl Iterator<Item = &KeyBinding> {
        let ty = action.action_type();
        self.bindings
            .iter()
            .filter(move |b| b.action.action_type() == ty)
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    pub fn clear_pending(&mut self) {
        self.pending.clear();
    }

    /// Feeds a keystroke. `contexts` are the key contexts active on the
    /// current focus path.
    pub fn feed(&mut self, keystroke: &Keystroke, contexts: &[&'static str]) -> KeymapMatch {
        let mut sequence = self.pending.clone();
        sequence.push(keystroke.clone());

        let active = |b: &KeyBinding| b.context.is_none_or(|c| contexts.contains(&c));
        let mut complete: Vec<(&KeyBinding, usize)> = Vec::new();
        let mut is_prefix = false;
        for (index, binding) in self.bindings.iter().enumerate() {
            if !active(binding) || binding.keystrokes.len() < sequence.len() {
                continue;
            }
            let prefix_matches = sequence
                .iter()
                .zip(&binding.keystrokes)
                .all(|(pressed, bound)| pressed.matches(bound));
            if !prefix_matches {
                continue;
            }
            if binding.keystrokes.len() == sequence.len() {
                complete.push((binding, index));
            } else {
                is_prefix = true;
            }
        }

        if is_prefix {
            self.pending = sequence;
            return KeymapMatch::Pending;
        }
        self.pending.clear();
        if complete.is_empty() {
            return KeymapMatch::None;
        }
        // Context bound bindings first, then later definitions first (user
        // bindings added after defaults override them).
        complete.sort_by_key(|(b, index)| (b.context.is_none(), std::cmp::Reverse(*index)));
        KeymapMatch::Matched(
            complete
                .into_iter()
                .map(|(b, _)| b.action.boxed_clone())
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Key, Modifiers};

    crate::actions!(Save, SaveAll, Undo, EditorUndo);

    fn ks(s: &str) -> Keystroke {
        Keystroke::parse(s).unwrap()
    }

    #[test]
    fn single_and_chorded_bindings() {
        let mut km = Keymap::new();
        km.add(KeyBinding::new("ctrl-s", Save));
        km.add(KeyBinding::new("ctrl-k s", SaveAll));

        match km.feed(&ks("ctrl-s"), &[]) {
            KeymapMatch::Matched(a) => assert_eq!(a[0].action_type(), TypeId::of::<Save>()),
            other => panic!("{other:?}"),
        }
        assert!(matches!(km.feed(&ks("ctrl-k"), &[]), KeymapMatch::Pending));
        match km.feed(&ks("s"), &[]) {
            KeymapMatch::Matched(a) => assert_eq!(a[0].action_type(), TypeId::of::<SaveAll>()),
            other => panic!("{other:?}"),
        }
        assert!(matches!(km.feed(&ks("x"), &[]), KeymapMatch::None));
    }

    #[test]
    fn context_bindings_take_precedence() {
        let mut km = Keymap::new();
        km.add(KeyBinding::new("ctrl-z", Undo));
        km.add(KeyBinding::new("ctrl-z", EditorUndo).context("Editor"));
        match km.feed(&ks("ctrl-z"), &["Editor"]) {
            KeymapMatch::Matched(a) => {
                assert_eq!(a[0].action_type(), TypeId::of::<EditorUndo>());
                assert_eq!(a[1].action_type(), TypeId::of::<Undo>());
            }
            other => panic!("{other:?}"),
        }
        match km.feed(&ks("ctrl-z"), &[]) {
            KeymapMatch::Matched(a) => assert_eq!(a.len(), 1),
            other => panic!("{other:?}"),
        }
        let _ = Keystroke::new(Modifiers::NONE, Key::Unidentified);
    }
}
