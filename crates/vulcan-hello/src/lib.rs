pub fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greet_alice() {
        assert_eq!(greet("Alice"), "Hello, Alice!");
    }

    #[test]
    fn greet_world() {
        assert_eq!(greet("World"), "Hello, World!");
    }
}
