use std::fmt::Display;

use thiserror::Error;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ProxyFormat {
    HostPortUsernamePassword,
}

#[derive(Debug, Clone)]
pub struct Proxy {
    host: String,
    port: u16,
    username: Option<String>,
    password: Option<String>,
}

#[derive(Error, Debug)]
pub enum ProxyParseError {
    #[error("invalid proxy part amount")]
    InvalidProxyPartAmountError,
    #[error("proxy port is not a number")]
    ProxyPortNaNError,
}

impl Proxy {
    pub fn new(
        host: String,
        port: u16,
        username: Option<String>,
        password: Option<String>,
    ) -> Proxy {
        Proxy {
            host,
            port,
            username,
            password,
        }
    }

    ///
    /// Parse a proxy from a string
    ///
    /// This method takes a proxy format and a string and returns a proxy.
    ///
    /// # Example
    /// ```rust
    /// use proxytester::{Proxy, ProxyFormat};
    ///
    /// let proxy = Proxy::from_str(ProxyFormat::HostPortUsernamePassword, "host:1234:username:password").unwrap();
    ///
    /// # assert_eq!(proxy.to_string(), "http://username:password@host:1234");
    /// ```
    ///
    pub fn from_str(format: ProxyFormat, line: &str) -> Result<Proxy, ProxyParseError> {
        match format {
            ProxyFormat::HostPortUsernamePassword => {
                let parts = line.split(':').collect::<Vec<_>>();
                if parts.len() != 4 {
                    return Err(ProxyParseError::InvalidProxyPartAmountError);
                }

                let host = parts[0].to_string();
                let port = parts[1]
                    .parse::<u16>()
                    .map_err(|_| ProxyParseError::ProxyPortNaNError)?;
                let username = Option::from(parts[2].to_owned());
                let password = Option::from(parts[3].to_owned());

                Ok(Proxy::new(host, port, username, password))
            }
        }
    }
}

impl Display for Proxy {
    ///
    /// Display a proxy
    ///
    /// This method returns a string representation of a proxy.
    /// The format is `http://username:password@host:port`.
    ///
    /// # Example
    /// ```rust
    /// use proxytester::{Proxy, ProxyFormat};
    ///
    /// let proxy = Proxy::from_str(ProxyFormat::HostPortUsernamePassword, "host:1234:username:password").unwrap();
    /// let proxy_string = format!("{}", proxy); // http://username:password@host:1234
    ///
    /// # assert_eq!(proxy_string, "http://username:password@host:1234");
    /// ```
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "http://{}:{}@{}:{}",
            self.username.as_ref().unwrap(),
            self.password.as_ref().unwrap(),
            self.host,
            self.port
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::Proxy;
    use crate::ProxyFormat;
    use crate::ProxyParseError;

    #[test]
    fn new_proxy_all_fields_test() {
        let proxy = Proxy::new(
            "host".to_string(),
            1234,
            Some("username".to_string()),
            Some("password".to_string()),
        );

        assert_eq!(proxy.host, "host");
        assert_eq!(proxy.port, 1234);
        assert_eq!(proxy.username, Some("username".to_string()));
        assert_eq!(proxy.password, Some("password".to_string()));
    }

    #[test]
    fn new_proxy_no_password_test() {
        let proxy = Proxy::new("host".to_string(), 1234, None, Some("password".to_string()));

        assert_eq!(proxy.host, "host");
        assert_eq!(proxy.port, 1234);
        assert_eq!(proxy.username, None);
        assert_eq!(proxy.password, Some("password".to_string()));
    }

    #[test]
    fn new_proxy_no_username_test() {
        let proxy = Proxy::new("host".to_string(), 1234, Some("username".to_string()), None);

        assert_eq!(proxy.host, "host");
        assert_eq!(proxy.port, 1234);
        assert_eq!(proxy.username, Some("username".to_string()));
        assert_eq!(proxy.password, None);
    }

    #[test]
    fn parse_proxy_all_fields_test() {
        let proxy = Proxy::from_str(
            ProxyFormat::HostPortUsernamePassword,
            "host:1234:username:password",
        )
        .unwrap();

        assert_eq!(proxy.host, "host");
        assert_eq!(proxy.port, 1234);
        assert_eq!(proxy.username, Some("username".to_string()));
        assert_eq!(proxy.password, Some("password".to_string()));
    }

    #[test]
    fn parse_proxy_no_password_test() {
        let proxy =
            Proxy::from_str(ProxyFormat::HostPortUsernamePassword, "host:1234:username:").unwrap();

        assert_eq!(proxy.host, "host");
        assert_eq!(proxy.port, 1234);
        assert_eq!(proxy.username, Some("username".to_string()));
        assert_eq!(proxy.password, Some("".to_string()));
    }

    #[test]
    fn parse_proxy_no_username_test() {
        let proxy =
            Proxy::from_str(ProxyFormat::HostPortUsernamePassword, "host:1234::password").unwrap();

        assert_eq!(proxy.host, "host");
        assert_eq!(proxy.port, 1234);
        assert_eq!(proxy.username, Some("".to_string()));
        assert_eq!(proxy.password, Some("password".to_string()));
    }

    #[test]
    fn parse_proxy_format_not_enough_parts_test() {
        if let Err(ProxyParseError::InvalidProxyPartAmountError) =
            Proxy::from_str(ProxyFormat::HostPortUsernamePassword, "host:1234")
        {
            return;
        }

        panic!("Expected InvalidProxyPartAmountError");
    }

    #[test]
    fn parse_proxy_port_nan_test() {
        if let Err(ProxyParseError::ProxyPortNaNError) =
            Proxy::from_str(ProxyFormat::HostPortUsernamePassword, "host:nan::")
        {
            return;
        }

        panic!("Expected ProxyPortNaNError");
    }

    #[test]
    fn format_proxy_all_fields_test() {
        let proxy = Proxy::from_str(
            ProxyFormat::HostPortUsernamePassword,
            "host:1234:username:password",
        )
        .unwrap();

        assert_eq!(format!("{}", proxy), "http://username:password@host:1234");
    }

    #[test]
    fn format_proxy_no_password_test() {
        let proxy =
            Proxy::from_str(ProxyFormat::HostPortUsernamePassword, "host:1234:username:").unwrap();

        assert_eq!(format!("{}", proxy), "http://username:@host:1234");
    }

    #[test]
    fn format_proxy_no_username_test() {
        let proxy =
            Proxy::from_str(ProxyFormat::HostPortUsernamePassword, "host:1234::password").unwrap();

        assert_eq!(format!("{}", proxy), "http://:password@host:1234");
    }
}
