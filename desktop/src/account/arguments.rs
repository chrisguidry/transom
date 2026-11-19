//! The app's command line, as what it asks of the account. The desktop
//! starts the app with a sign-in's callback as its one argument, and a
//! second start hands its arguments to the instance already running.

use super::Command;

/// What `arguments`, a command line without the program's name, asks of
/// the account, and whether it asks for no window. `--sign-in <address>`
/// begins signing in to that server, as the sign-in screen's button does.
pub fn heed(arguments: &[String]) -> (Vec<Command>, bool) {
    let background = arguments.iter().any(|argument| argument == "--background");
    let mut asked = Vec::new();
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        if argument.starts_with("sh.transom:") {
            asked.push(Command::Callback(argument.clone()));
        } else if argument == "--sign-in"
            && let Some(server) = arguments.next()
        {
            asked.push(Command::SignIn(server.clone()));
        }
    }
    (asked, background)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heeded(arguments: &[&str]) -> (Vec<Command>, bool) {
        let arguments: Vec<String> = arguments
            .iter()
            .map(|&argument| argument.to_owned())
            .collect();
        heed(&arguments)
    }

    #[test]
    fn no_arguments_ask_for_the_window() {
        assert_eq!(heeded(&[]), (vec![], false));
    }

    #[test]
    fn background_asks_for_no_window() {
        assert_eq!(heeded(&["--background"]), (vec![], true));
    }

    #[test]
    fn a_callback_is_handed_to_the_account() {
        let callback = "sh.transom:/callback?code=abc";
        let asked = vec![Command::Callback(callback.to_owned())];
        assert_eq!(heeded(&[callback]), (asked, false));
    }

    #[test]
    fn sign_in_takes_the_address_after_it() {
        let asked = vec![Command::SignIn("transom.example.com".to_owned())];
        let arguments = ["--background", "--sign-in", "transom.example.com"];
        assert_eq!(heeded(&arguments), (asked, true));
    }

    #[test]
    fn sign_in_without_an_address_and_anything_else_ask_nothing() {
        assert_eq!(heeded(&["--verbose", "--sign-in"]), (vec![], false));
    }
}
