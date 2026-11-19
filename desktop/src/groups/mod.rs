//! A group's members: who they are, removing one, and inviting another.
//! Members are peers, so any member may do each of these.

pub mod screen;

/// The handle a person typed to invite, with its `@`, or `None` when
/// they typed nothing.
pub fn handle(typed: &str) -> Option<String> {
    let name = typed.trim().trim_start_matches('@');
    (!name.is_empty()).then(|| format!("@{name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_typed_name_becomes_a_handle() {
        assert_eq!(handle(" sam ").as_deref(), Some("@sam"));
        assert_eq!(handle("@sam").as_deref(), Some("@sam"));
        assert_eq!(handle(" @ "), None);
    }
}
