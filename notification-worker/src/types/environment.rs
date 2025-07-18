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

    /// Returns the XMTP gRPC endpoint for this environment
    #[must_use]
    pub fn xmtp_endpoint(&self) -> &'static str {
        match self {
            Self::Production => "https://grpc.production.xmtp.network:443",
            Self::Staging => "https://grpc.dev.xmtp.network:443",
            Self::Development => "http://localhost:25556", // Local Docker
        }
    }

    /// Returns whether to use TLS for this environment
    #[must_use]
    pub fn use_tls(&self) -> bool {
        match self {
            Self::Production | Self::Staging => true,
            Self::Development => false,
        }
    }

    /// Returns the default number of workers for this environment
    #[must_use]
    pub fn default_num_workers(&self) -> usize {
        match self {
            Self::Production => 50,
            Self::Staging => 20,
            Self::Development => 10,
        }
    }
}

#[cfg(test)]
mod tests {
    use serial_test::serial;

    use super::*;

    #[test]
    #[serial]
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
    #[serial]
    #[should_panic(expected = "Invalid environment: invalid")]
    fn test_invalid_environment() {
        env::set_var("APP_ENV", "invalid");
        let _ = Environment::from_env();
        env::remove_var("APP_ENV");
    }
}
