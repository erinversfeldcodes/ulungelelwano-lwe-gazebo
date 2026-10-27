//! Domain logic for the flashcard app, kept free of web/framework dependencies
//! so it compiles (and is cached) independently of the `web` crate.

pub fn greeting() -> &'static str {
    "Hello, world!"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greeting_says_hello() {
        assert_eq!(greeting(), "Hello, world!");
    }
}
