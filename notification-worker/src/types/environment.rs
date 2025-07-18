//! Environment configuration for different deployment stages

use std::env;

/// Application environment configuration
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Environment {
    /// Production environment
    Production,
    /// Staging environment  
    Staging,
    /// Development environment (uses LocalStack)
    Development,
}

impl Environment {
    /// Creates an Environment from the `APP_ENV` environment variable
    ///
    /// # Panics
    ///
    /// Panics if `APP_ENV` contains an invalid value
    #[must_use]
    pub fn from_env() -> Self {
        let env = env::var("APP_ENV")
            .unwrap_or_else(|_| "development".to_string())
            .trim()
            .to_lowercase();

        match env.as_str() {
            "production" => Self::Production,
            "staging" => Self::Staging,
            "development" => Self::Development,
            _ => panic!("Invalid environment: {env}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_environment_from_env() {
        // Test development (default)
        env::remove_var("APP_ENV");
        assert_eq!(Environment::from_env(), Environment::Development);

        // Test explicit development
        env::set_var("APP_ENV", "development");
        assert_eq!(Environment::from_env(), Environment::Development);

        // Test staging
        env::set_var("APP_ENV", "staging");
        assert_eq!(Environment::from_env(), Environment::Staging);

        // Test production
        env::set_var("APP_ENV", "production");
        assert_eq!(Environment::from_env(), Environment::Production);

        // Cleanup
        env::remove_var("APP_ENV");
    }

    #[test]
    #[should_panic(expected = "Invalid environment: invalid")]
    fn test_invalid_environment() {
        env::set_var("APP_ENV", "invalid");
        let _ = Environment::from_env();
        env::remove_var("APP_ENV");
    }
}
