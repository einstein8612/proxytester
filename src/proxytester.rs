use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
    sync::Arc,
    time::Duration,
};

use crate::{Proxy, ProxyFormat, ProxyParseError};

use curl::easy::Easy;
use tokio::{
    sync::{
        mpsc::{self, Receiver},
        Semaphore,
    },
    time::Instant,
};

use thiserror::Error;

#[derive(Debug)]
pub struct ProxyTesterOptions {
    format: Option<ProxyFormat>,
    workers: Option<usize>,
    timeout: Option<Duration>,
    url: Option<String>,
}

#[derive(Debug)]
pub struct ProxyTester {
    format: ProxyFormat,
    workers: usize,
    timeout: Duration,
    url: String,

    proxies: Vec<Proxy>,
}

#[derive(Error, Debug)]
pub enum ProxyTestError {
    #[error("some unknown error happened")]
    UnknownError,

    #[error("semaphore acquire error: {0}")]
    SemaphoreAcquireError(#[from] tokio::sync::AcquireError),

    #[error("curl error: {0}")]
    CurlError(#[from] curl::Error),
}

#[derive(Debug)]
pub struct ProxyTestSuccess {
    pub duration: Duration,
}

#[derive(Debug)]
pub struct ProxyTest {
    pub proxy: Proxy,
    pub result: Result<ProxyTestSuccess, ProxyTestError>,
}

const CHANNEL_SIZE: usize = 100;

impl ProxyTester {
    ///
    /// Create a new ProxyTesterOptions which is the builder for the ProxyTester
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::time::Duration;
    /// use proxytester::{ProxyTester, ProxyFormat};
    ///
    /// let mut proxy_tester = ProxyTester::builder()
    ///     .set_format(ProxyFormat::HostPortUsernamePassword)
    ///     .set_url("https://example.com".to_owned())
    ///     .set_workers(10)
    ///     .set_timeout(Duration::from_millis(5000))
    ///     .build();
    /// ```
    pub fn builder() -> ProxyTesterOptions {
        ProxyTesterOptions::new()
    }

    ///
    /// Load proxies from a file
    /// The file should contain one proxy per line
    /// The format of the proxy should match the format of the ProxyTester
    ///
    /// # Examples
    ///
    /// Load proxies from a file using String
    /// ```rust
    /// use proxytester::ProxyTesterOptions;
    ///
    /// let mut proxy_tester = ProxyTesterOptions::default().build();
    /// proxy_tester.load_from_file("testdata/test_proxies.txt");
    ///
    /// assert_eq!(proxy_tester.len(), 10);
    /// ```
    ///
    /// Load proxies from a file using Path
    /// ```rust
    /// use proxytester::ProxyTesterOptions;
    /// use std::path::Path;
    ///
    /// let proxy_file_path = Path::new("testdata/test_proxies.txt");
    ///
    /// let mut proxy_tester = ProxyTesterOptions::default().build();
    /// proxy_tester.load_from_file(proxy_file_path);
    ///
    /// assert_eq!(proxy_tester.len(), 10);
    /// ```
    pub fn load_from_file<P: AsRef<Path>>(&mut self, path: P) -> Result<(), ProxyParseError> {
        let file = File::open(path).unwrap();
        let buf_reader = BufReader::new(file);
        let out = buf_reader.lines().map(|line| {
            let line = line.unwrap();
            Proxy::from_str(self.format, &line).unwrap()
        });
        self.proxies.extend(out);
        Ok(())
    }

    ///
    /// Run the proxy tester based on the loaded proxies
    /// Returns a vector of results
    ///
    pub async fn run(&mut self) -> Receiver<ProxyTest> {
        // Clone and wrap in Arc the URL and semaphore to be used in the async block
        let url = Arc::new(self.url.clone());
        let semaphore = Arc::new(Semaphore::new(self.workers));
        let timeout = self.timeout;

        // Create a vector to store the handles of the async blocks
        let mut handles = Vec::with_capacity(self.proxies.len());

        // Create a channel to send the results back
        let (sender, receiver) = mpsc::channel(CHANNEL_SIZE);

        // Iterate over the proxies and spawn an async block for each
        for proxy in self.proxies.clone() {
            let url = url.clone();
            let semaphore = semaphore.clone();
            let sender = sender.clone(); // Should be cheap like Arc clones

            let handle = tokio::spawn(async move {
                // Acquire a permit from the semaphore
                // This will block if the semaphore is at capacity (worker count)
                // Once the async block is finished, the permit is released
                let _permit = semaphore
                    .acquire()
                    .await
                    .expect("semaphore was poisoned, this should never happen");

                let proxy_string = proxy.to_string();
                let result = tokio::task::spawn_blocking(move || {
                    let now = Instant::now();

                    // Create a Curl client
                    let mut easy = Easy::new();
                    easy.url(&url)?;
                    // Set the proxy
                    easy.proxy(&proxy_string)?;
                    // Set the timeout
                    easy.timeout(timeout)?;

                    // We don't care about the response, we just want to test the proxy
                    easy.write_function(|data| Ok(data.len()))?;

                    // Perform the request
                    easy.perform()?;

                    Ok(ProxyTestSuccess {
                        duration: now.elapsed(),
                    })
                })
                .await
                .expect("join error, this should never happen");

                sender.send(ProxyTest { proxy, result }).await.unwrap();
            });

            // Push the handle to the vector
            handles.push(handle);
        }

        // Join all the handles and wait for them to finish
        tokio::spawn(async move {
            futures::future::join_all(handles).await;

            // Drop the sender to close the receiver
            drop(sender);
        });

        receiver
    }

    ///
    /// Get the amount of proxies loaded
    ///
    pub fn len(&self) -> usize {
        self.proxies.len()
    }

    ///
    /// Check if the proxy tester is empty
    ///
    pub fn is_empty(&self) -> bool {
        self.proxies.is_empty()
    }

    ///
    /// Get the url that the proxies will be tested against
    ///
    pub fn url(&self) -> &str {
        &self.url
    }

    ///
    /// Get the workers count
    ///
    pub fn workers(&self) -> usize {
        self.workers
    }

    ///
    /// Get the timeout duration
    ///
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

impl ProxyTesterOptions {
    ///
    /// Create a new ProxyTesterOptions
    ///
    /// This is the builder for the ProxyTester.
    ///
    /// # Examples
    /// ```rust
    /// use proxytester::{ProxyTesterOptions, ProxyFormat};
    /// use std::time::Duration;
    ///
    /// let proxy_tester = ProxyTesterOptions::new()
    ///     .set_format(ProxyFormat::HostPortUsernamePassword)
    ///     .set_url("https://example.com".to_owned())
    ///     .set_workers(10)
    ///     .set_timeout(Duration::from_millis(5000))
    ///     .build();
    /// ```
    pub fn new() -> ProxyTesterOptions {
        ProxyTesterOptions {
            format: None,
            workers: None,
            timeout: None,
            url: None,
        }
    }

    ///
    /// Set the format of the proxies
    ///
    /// This is a fluent setter method which must be chained or used as it consumes self.
    ///
    /// See [ProxyTesterOptions](struct.ProxyTesterOptions.html) for more information.
    ///
    pub fn set_format(mut self, format: ProxyFormat) -> Self {
        self.format = Option::from(format);
        self
    }

    ///
    /// Set the amount of workers
    ///
    /// This is a fluent setter method which must be chained or used as it consumes self.
    ///
    /// See [ProxyTesterOptions](struct.ProxyTesterOptions.html) for more information.
    ///
    pub fn set_workers(mut self, workers: usize) -> Self {
        self.workers = Option::from(workers);
        self
    }

    ///
    /// Set the timeout duration
    ///
    /// This is a fluent setter method which must be chained or used as it consumes self.
    ///
    /// See [ProxyTesterOptions](struct.ProxyTesterOptions.html) for more information.
    ///
    pub fn set_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Option::from(timeout);
        self
    }

    ///
    /// Set the URL that the proxies will be tested against
    ///
    /// This is a fluent setter method which must be chained or used as it consumes self.
    ///
    /// See [ProxyTesterOptions](struct.ProxyTesterOptions.html) for more information.
    ///
    pub fn set_url(mut self, url: String) -> Self {
        self.url = Option::from(url);
        self
    }

    ///
    /// Build the ProxyTester
    ///
    /// This is a fluent setter method which must be chained or used as it consumes self.
    ///
    /// See [ProxyTesterOptions](struct.ProxyTesterOptions.html) for more information.
    ///
    pub fn build(self) -> ProxyTester {
        ProxyTester {
            format: self.format.expect("Format is required"),
            workers: self.workers.expect("Workers is required"),
            timeout: self.timeout.expect("Timeout is required"),
            url: self.url.clone().expect("URL is required"),

            proxies: Vec::new(),
        }
    }
}

impl Default for ProxyTesterOptions {
    fn default() -> Self {
        ProxyTesterOptions {
            format: Option::from(ProxyFormat::HostPortUsernamePassword),
            workers: Option::from(5),
            timeout: Option::from(Duration::from_millis(5000)),
            url: Option::from("https://google.com".to_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write;
    use std::panic;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::time::Duration;

    use http_test_server::http::Status;
    use http_test_server::TestServer;
    use tempdir::TempDir;

    use crate::ProxyFormat;
    use crate::ProxyTestError;
    use crate::ProxyTester;
    use crate::ProxyTesterOptions;

    #[test]
    fn default_proxy_tester_options() {
        let proxy = ProxyTesterOptions::default();

        assert_eq!(proxy.format, Some(ProxyFormat::HostPortUsernamePassword));
        assert_eq!(proxy.url, Some("https://google.com".to_owned()));
        assert_eq!(proxy.workers, Some(5));
        assert_eq!(proxy.timeout, Some(Duration::from_millis(5000)));
    }

    #[test]
    #[should_panic]
    fn proxy_tester_options_must_include_format() {
        panic::set_hook(Box::new(|_info| {
            // do nothing
        }));

        ProxyTesterOptions::new().build();
    }

    #[test]
    #[should_panic]
    fn proxy_tester_options_must_include_workers() {
        panic::set_hook(Box::new(|_info| {
            // do nothing
        }));

        ProxyTesterOptions::new()
            .set_format(ProxyFormat::HostPortUsernamePassword)
            .build();
    }

    #[test]
    #[should_panic]
    fn proxy_tester_options_must_include_timeout() {
        panic::set_hook(Box::new(|_info| {
            // do nothing
        }));

        ProxyTesterOptions::new()
            .set_format(ProxyFormat::HostPortUsernamePassword)
            .set_workers(5)
            .build();
    }

    #[test]
    #[should_panic]
    fn proxy_tester_options_must_include_url() {
        panic::set_hook(Box::new(|_info| {
            // do nothing
        }));

        ProxyTesterOptions::new()
            .set_format(ProxyFormat::HostPortUsernamePassword)
            .set_workers(5)
            .set_timeout(Duration::from_secs(5))
            .build();
    }

    #[test]
    fn proxy_tester_options_build() {
        let proxy_tester = ProxyTesterOptions::new()
            .set_format(ProxyFormat::HostPortUsernamePassword)
            .set_workers(5)
            .set_timeout(Duration::from_secs(5))
            .set_url("https://google.com".to_owned())
            .build();

        assert_eq!(proxy_tester.format, ProxyFormat::HostPortUsernamePassword);
        assert_eq!(proxy_tester.workers(), 5);
        assert_eq!(proxy_tester.timeout(), Duration::from_secs(5));
        assert_eq!(proxy_tester.url(), "https://google.com".to_owned());
    }

    #[test]
    fn proxy_tester_builder_exposure_method() {
        let proxy_tester = ProxyTester::builder()
            .set_format(ProxyFormat::HostPortUsernamePassword)
            .set_workers(5)
            .set_timeout(Duration::from_secs(5))
            .set_url("https://google.com".to_owned())
            .build();

        assert_eq!(proxy_tester.format, ProxyFormat::HostPortUsernamePassword);
        assert_eq!(proxy_tester.workers(), 5);
        assert_eq!(proxy_tester.timeout(), Duration::from_secs(5));
        assert_eq!(proxy_tester.url(), "https://google.com".to_owned());
    }

    #[test]
    fn proxy_tester_starts_empty() {
        let proxy_tester = ProxyTesterOptions::default().build();

        assert!(proxy_tester.is_empty());
    }

    #[test]
    fn proxy_tester_load_from_file() {
        let mut proxy_tester = ProxyTesterOptions::default().build();

        proxy_tester
            .load_from_file("testdata/test_proxies.txt")
            .unwrap();

        assert_eq!(proxy_tester.len(), 10);
    }

    #[tokio::test]
    async fn proxy_tester_run_broken_proxy() {
        let mut proxy_tester = ProxyTesterOptions::default()
            .set_timeout(Duration::from_millis(100))
            .build();

        // Must return tempdir to keep it alive
        let (file_path, _tempdir) = create_temp_file("nonexistent:1234:username:password");
        proxy_tester.load_from_file(file_path).unwrap();

        let mut receiver = proxy_tester.run().await;
        let received = receiver.recv().await.unwrap();

        if let Err(ProxyTestError::CurlError(_err)) = received.result {
            return;
        }

        panic!("Expected CurlError");
    }

    #[tokio::test]
    async fn proxy_tester_run_multiple_broken_proxy() {
        let mut proxy_tester = ProxyTesterOptions::default()
            .set_workers(1)
            .set_timeout(Duration::from_millis(100))
            .build();

        // Must return tempdir to keep it alive
        let (file_path, _tempdir) = create_temp_file("nonexistent:1234:username:password\nnonexistent:1234:username:password\nnonexistent:1234:username:password");
        proxy_tester.load_from_file(file_path).unwrap();

        let mut receiver = proxy_tester.run().await;

        for _ in 0..3 {
            let received = receiver.recv().await.unwrap();

            if let Err(ProxyTestError::CurlError(_err)) = received.result {
                continue;
            }

            panic!("Expected CurlError");
        }
    }

    #[tokio::test]
    async fn proxy_tester_run_good_proxy() {
        let mut proxy_tester = ProxyTesterOptions::default()
            .set_timeout(Duration::from_millis(1000))
            .set_url("http://1.1.1.1".to_owned())
            .build();

        let proxy_used = Arc::from(Mutex::from(false));
        let proxy_used_clone = proxy_used.clone();

        // Setup local fake proxy
        let server = TestServer::new().unwrap();
        let resource = server.create_resource("/");

        resource.status(Status::Created).body_fn(move |_params| {
            let mut x = proxy_used_clone.lock().expect("lock poisoned");
            *x = true;

            "SUCCESS".to_owned()
        });

        // Must return tempdir to keep it alive
        let (file_path, _tempdir) = create_temp_file(&format!("localhost:{}::", server.port()));
        proxy_tester.load_from_file(file_path).unwrap();

        let mut receiver = proxy_tester.run().await;

        // Wait for the response
        let received = receiver.recv().await.unwrap();
        received.result.expect("proxy test success");

        assert!(*proxy_used.lock().expect("lock poisoned"));
    }

    fn create_temp_file(content: &str) -> (PathBuf, TempDir) {
        let tmp_dir = TempDir::new("proxytester_test_data").expect("create temp dir");
        let file_path = tmp_dir.path().join("proxies.txt");
        let mut tmp_file = File::create(file_path.clone()).expect("create temp file");
        writeln!(tmp_file, "{}", content).expect("write to file");

        (file_path, tmp_dir)
    }
}
