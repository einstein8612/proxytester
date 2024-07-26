use std::{
    io::{self, stdout, Stdout},
    path::PathBuf,
    time::Duration,
};

use clap::Parser;
use proxytester::{ProxyTest, ProxyTesterOptions};
use ratatui::{
    crossterm::{
        event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
        ExecutableCommand,
    },
    prelude::*,
    widgets::*,
};
use tokio::{select, sync::mpsc::Receiver};

const POLL_DURATION: Duration = Duration::from_millis(50);

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The URL to test the proxies against
    #[arg(short, long, default_value = "https://1.1.1.1")]
    url: String,

    /// How many workers to use,
    /// ergo how many proxies to test at once
    #[arg(short, long, default_value_t = 1)]
    workers: usize,

    /// Timeout for each request in milliseconds
    #[arg(short, long = "timeout", default_value_t = 5000)]
    timeout_ms: u64,

    /// File to read the proxies from
    #[arg(required = true)]
    files: Vec<PathBuf>,
}

///
/// Initialize the UI
///
#[cfg(not(tarpaulin_include))] // Ignored since it involves actual terminal
fn init_ui() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;

    Terminal::new(CrosstermBackend::new(stdout()))
}

///
/// Cleanup the UI
///
#[cfg(not(tarpaulin_include))] // Ignored since it involves actual terminal
fn cleanup_ui(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

struct AppState {
    workers: usize,
    timeout: Duration,
    url: String,
    proxy_count: usize,

    proxy_test_recv: Receiver<ProxyTest>,
    results_buffer: Vec<ProxyTest>,
}

struct App {
    state: AppState,
    selected_proxy: usize,
    exit: bool,
}

impl App {
    ///
    /// Run the application
    ///
    #[cfg(not(tarpaulin_include))] // Ignored since it's a never ending loop
    pub async fn run(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> io::Result<()> {
        while !self.exit {
            // Wait for an event to occur or a new ProxyTest to be available
            select! {
                // Wait for an event to occur
                _ = App::wait_for_event() => {
                    // Handle the event
                    self.handle_events()?;
                },
                // Wait for a new ProxyTest to be available
                Some(proxy_test) = self.state.proxy_test_recv.recv() => {
                    // Push them to the results buffer
                    self.state.results_buffer.push(proxy_test);
                },
            }

            // Draw the terminal
            terminal.draw(|frame| self.render_frame(frame))?;
        }

        Ok(())
    }

    ///
    /// Wait for an event to occur
    ///
    /// Simpel wrapper around the `event::poll` function
    /// to use it in an async context like the `select!` macro
    ///
    #[cfg(not(tarpaulin_include))] // Ignored since it's a util func
    async fn wait_for_event() -> io::Result<()> {
        loop {
            let res = tokio::task::spawn_blocking(|| event::poll(POLL_DURATION)).await??;
            if res {
                break;
            }
        }
        Ok(())
    }

    ///
    /// Handle the events
    ///
    /// This function will handle the events that are available,
    /// propogating the events to the correct handler.
    ///
    #[cfg(not(tarpaulin_include))] // Ignored since it involves keyboard input
    fn handle_events(&mut self) -> io::Result<()> {
        // We assume that an event is available
        // as we waited for an event to occur
        match event::read()? {
            // it's important to check that the event is a key press event as
            // crossterm also emits key release and repeat events on Windows.
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                self.handle_key_event(key_event)
            }
            _ => {}
        };
        Ok(())
    }

    ///
    /// Handle the key events
    ///
    /// This function will handle the key events, and update the state accordingly.
    ///
    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Esc | KeyCode::Char('q') => self.exit(),
            KeyCode::Down | KeyCode::Char('k') => {
                // Check if the selected proxy is the last one
                if self.selected_proxy >= self.state.results_buffer.len() - 1 {
                    self.selected_proxy = 0;
                } else {
                    self.selected_proxy += 1;
                }
            }
            KeyCode::Up | KeyCode::Char('i') => {
                // Check if the selected proxy is the first one
                if self.selected_proxy == 0 {
                    self.selected_proxy = self.state.results_buffer.len() - 1;
                } else {
                    self.selected_proxy -= 1;
                }
            }
            _ => {}
        }
    }

    ///
    /// Render the frame
    ///
    #[cfg(not(tarpaulin_include))] // Ignored since it's a util func
    fn render_frame(&self, frame: &mut Frame) {
        frame.render_widget(self, frame.size());
    }

    ///
    /// Exit the application
    ///
    /// This will set the exit flag to true and close the proxy_test_recv channel.
    ///
    fn exit(&mut self) {
        self.exit = true;
        self.state.proxy_test_recv.close();
    }
}

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let main_layout = Layout::new(
            Direction::Vertical,
            [
                Constraint::Length(7),
                Constraint::Length(3),
                Constraint::Min(0),
            ],
        )
        .split(area);

        let info_block = Block::new()
            .border_type(BorderType::Plain)
            .borders(Borders::all())
            .title("ProxyTester-Information");

        Paragraph::new(Text::from(vec![
            Line::from(format!("Proxies: {}", self.state.proxy_count)),
            Line::from(format!("URL: {}", self.state.url)),
            Line::from(format!("Workers: {}", self.state.workers)),
            Line::from(format!("Timeout: {:?}", self.state.timeout)),
            Line::from(format!("Version: v{}", env!("CARGO_PKG_VERSION"))),
        ]))
        .block(info_block)
        .render(main_layout[0], buf);

        Gauge::default()
            .block(Block::new().borders(Borders::all()).title("Progress"))
            .gauge_style(Color::White)
            .ratio(self.state.results_buffer.len() as f64 / self.state.proxy_count as f64)
            .label(Span::styled(
                format!(
                    "{}/{}",
                    self.state.results_buffer.len(),
                    self.state.proxy_count
                ),
                Style::new().italic().bold().fg(Color::DarkGray),
            ))
            .use_unicode(true)
            .render(main_layout[1], buf);

        let result_rows = self
            .state
            .results_buffer
            .iter()
            .map(|result| {
                let cells = match &result.result {
                    Ok(proxy_test_success) => vec![
                        result.proxy.to_string(),
                        "Success".to_string(),
                        format!("{:.3?}", proxy_test_success.duration),
                    ],
                    Err(err) => vec![result.proxy.to_string(), err.to_string(), "N/A".to_string()],
                };
                Row::new(cells)
            })
            .collect::<Vec<_>>();

        let selected_style = Style::default().fg(Color::DarkGray);

        let result_table = Table::new(
            result_rows,
            [Constraint::Min(1), Constraint::Min(1), Constraint::Max(10)],
        )
        .highlight_style(selected_style)
        .highlight_symbol(" * ")
        .highlight_spacing(HighlightSpacing::Always);

        let results_block = Block::new()
            .border_type(BorderType::Plain)
            .borders(Borders::all())
            .title("Test-Results");

        StatefulWidget::render(
            result_table.block(results_block),
            main_layout[2],
            buf,
            &mut TableState::default().with_selected(self.selected_proxy),
        );

        Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│"))
            .render(
                main_layout[2].inner(Margin {
                    vertical: 1,
                    horizontal: 0,
                }),
                buf,
                &mut ScrollbarState::new(self.state.results_buffer.len())
                    .position(self.selected_proxy),
            );
    }
}

#[tokio::main]
#[cfg(not(tarpaulin_include))] // Ignored since it's the main function
async fn main() -> io::Result<()> {
    // Parse the command line arguments
    let args = Args::parse();

    // Create a new proxy tester
    let mut proxy_tester = ProxyTesterOptions::default()
        .set_url(args.url.clone())
        .set_workers(args.workers)
        .set_timeout(Duration::from_millis(args.timeout_ms))
        .build();

    // Load the proxies from the files
    println!("Loading {} files", args.files.len());
    for file in args.files {
        proxy_tester
            .load_from_file(&file)
            .expect("Failed to load proxies from file");
    }

    // Check if there are any proxies loaded
    if proxy_tester.is_empty() {
        println!("No proxies loaded, you can't test nothing");
        return Ok(());
    }

    // Run the proxy tester
    let recv = proxy_tester.run().await;

    // Create the TUI app
    let mut app = App {
        state: AppState {
            workers: proxy_tester.workers(),
            timeout: proxy_tester.timeout(),
            url: proxy_tester.url().to_string(),
            proxy_count: proxy_tester.len(),

            results_buffer: Vec::with_capacity(proxy_tester.len()),
            proxy_test_recv: recv,
        },
        selected_proxy: 0,
        exit: false,
    };

    // Initialize the terminal
    let mut terminal = init_ui().expect("something went wrong with acquiring the terminal");

    app.run(&mut terminal).await?;

    cleanup_ui(&mut terminal)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use backend::TestBackend;
    use event::{KeyEventState, KeyModifiers};
    use proxytester::{Proxy, ProxyFormat, ProxyTestError};

    use super::*;

    #[test]
    fn proxytester_information() {
        let backend = TestBackend::new(25, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        let app = App {
            state: AppState {
                workers: 5,
                timeout: Duration::from_secs(5),
                url: "https://google.com".to_string(),
                proxy_count: 10,

                results_buffer: Vec::new(),
                proxy_test_recv: tokio::sync::mpsc::channel(1).1,
            },
            selected_proxy: 0,
            exit: false,
        };

        terminal
            .draw(|frame| frame.render_widget(&app, frame.size()))
            .unwrap();

        let mut expected = Buffer::with_lines([
            "┌ProxyTester-Information┐",
            "│Proxies: 10            │",
            "│URL: https://google.com│",
            "│Workers: 5             │",
            "│Timeout: 5s            │",
            "│Version: v0.1.0        │",
            "└───────────────────────┘",
            "┌Progress───────────────┐",
            "│         0/10          │",
            "└───────────────────────┘",
        ]);
        // Set the colors for the progress bar
        for x in 1..=23 {
            expected.get_mut(x, 8).set_fg(Color::White);
        }
        // Set the modifiers for the progress label
        for x in 10..=13 {
            expected
                .get_mut(x, 8)
                .set_style(Style::new().bold().italic().fg(Color::DarkGray));
        }
        terminal.backend().assert_buffer(&expected);
    }

    #[test]
    fn progress_bar_filled() {
        let backend = TestBackend::new(25, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        let app = App {
            state: AppState {
                workers: 5,
                timeout: Duration::from_secs(5),
                url: "https://google.com".to_string(),
                proxy_count: 10,

                results_buffer: vec![ProxyTest {
                    proxy: Proxy::from_str(
                        ProxyFormat::HostPortUsernamePassword,
                        "host:1234:username:password",
                    )
                    .unwrap(),
                    result: Ok(proxytester::ProxyTestSuccess {
                        duration: Duration::from_secs(1),
                    }),
                }],
                proxy_test_recv: tokio::sync::mpsc::channel(1).1,
            },
            selected_proxy: 1,
            exit: false,
        };

        terminal
            .draw(|frame| frame.render_widget(&app, frame.size()))
            .unwrap();

        let mut expected = Buffer::with_lines([
            "┌ProxyTester-Information┐",
            "│Proxies: 10            │",
            "│URL: https://google.com│",
            "│Workers: 5             │",
            "│Timeout: 5s            │",
            "│Version: v0.1.0        │",
            "└───────────────────────┘",
            "┌Progress───────────────┐",
            "│██▎      1/10          │",
            "└───────────────────────┘",
        ]);
        // Set the colors for the progress bar
        for x in 1..=23 {
            expected.get_mut(x, 8).set_fg(Color::White);
        }
        // Set the modifiers for the progress label
        for x in 10..=13 {
            expected
                .get_mut(x, 8)
                .set_style(Style::new().bold().italic().fg(Color::DarkGray));
        }
        terminal.backend().assert_buffer(&expected);
    }

    #[test]
    fn proxy_results_displays_success() {
        let backend = TestBackend::new(100, 13);
        let mut terminal = Terminal::new(backend).unwrap();

        let app = App {
            state: AppState {
                workers: 5,
                timeout: Duration::from_secs(5),
                url: "https://google.com".to_string(),
                proxy_count: 10,

                results_buffer: vec![ProxyTest {
                    proxy: Proxy::from_str(
                        ProxyFormat::HostPortUsernamePassword,
                        "host:1234:username:password",
                    )
                    .unwrap(),
                    result: Ok(proxytester::ProxyTestSuccess {
                        duration: Duration::from_secs(1),
                    }),
                }],
                proxy_test_recv: tokio::sync::mpsc::channel(1).1,
            },
            selected_proxy: 1,
            exit: false,
        };

        terminal
            .draw(|frame| frame.render_widget(&app, frame.size()))
            .unwrap();

        let mut expected = Buffer::with_lines([
            "┌ProxyTester-Information───────────────────────────────────────────────────────────────────────────┐",
            "│Proxies: 10                                                                                       │",
            "│URL: https://google.com                                                                           │",
            "│Workers: 5                                                                                        │",
            "│Timeout: 5s                                                                                       │",
            "│Version: v0.1.0                                                                                   │",
            "└──────────────────────────────────────────────────────────────────────────────────────────────────┘",
            "┌Progress──────────────────────────────────────────────────────────────────────────────────────────┐",
            "│█████████▊                                     1/10                                               │",
            "└──────────────────────────────────────────────────────────────────────────────────────────────────┘",
            "┌Test-Results──────────────────────────────────────────────────────────────────────────────────────┐",
            "│   http://username:password@host:1234         Success                                   1.000s    █",
            "└──────────────────────────────────────────────────────────────────────────────────────────────────┘",
        ]);
        // Set the colors for the progress bar
        for x in 1..=98 {
            expected.get_mut(x, 8).set_fg(Color::White);
        }
        // Set the modifiers for the progress label
        for x in 48..=51 {
            expected
                .get_mut(x, 8)
                .set_style(Style::new().bold().italic().fg(Color::DarkGray));
        }
        terminal.backend().assert_buffer(&expected);
    }

    #[test]
    fn proxy_results_displays_error() {
        let backend = TestBackend::new(100, 13);
        let mut terminal = Terminal::new(backend).unwrap();

        let app = App {
            state: AppState {
                workers: 5,
                timeout: Duration::from_secs(5),
                url: "https://google.com".to_string(),
                proxy_count: 10,

                results_buffer: vec![ProxyTest {
                    proxy: Proxy::from_str(
                        ProxyFormat::HostPortUsernamePassword,
                        "host:1234:username:password",
                    )
                    .unwrap(),
                    result: Err(ProxyTestError::UnknownError),
                }],
                proxy_test_recv: tokio::sync::mpsc::channel(1).1,
            },
            selected_proxy: 1,
            exit: false,
        };

        terminal
            .draw(|frame| frame.render_widget(&app, frame.size()))
            .unwrap();

        let mut expected = Buffer::with_lines([
            "┌ProxyTester-Information───────────────────────────────────────────────────────────────────────────┐",
            "│Proxies: 10                                                                                       │",
            "│URL: https://google.com                                                                           │",
            "│Workers: 5                                                                                        │",
            "│Timeout: 5s                                                                                       │",
            "│Version: v0.1.0                                                                                   │",
            "└──────────────────────────────────────────────────────────────────────────────────────────────────┘",
            "┌Progress──────────────────────────────────────────────────────────────────────────────────────────┐",
            "│█████████▊                                     1/10                                               │",
            "└──────────────────────────────────────────────────────────────────────────────────────────────────┘",
            "┌Test-Results──────────────────────────────────────────────────────────────────────────────────────┐",
            "│   http://username:password@host:1234         some unknown error happened               N/A       █",
            "└──────────────────────────────────────────────────────────────────────────────────────────────────┘",
        ]);
        // Set the colors for the progress bar
        for x in 1..=98 {
            expected.get_mut(x, 8).set_fg(Color::White);
        }
        // Set the modifiers for the progress label
        for x in 48..=51 {
            expected
                .get_mut(x, 8)
                .set_style(Style::new().bold().italic().fg(Color::DarkGray));
        }
        terminal.backend().assert_buffer(&expected);
    }

    #[test]
    fn proxy_results_should_be_scrollable() {
        let backend = TestBackend::new(100, 13);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut app = App {
            state: AppState {
                workers: 5,
                timeout: Duration::from_secs(5),
                url: "https://google.com".to_string(),
                proxy_count: 10,

                results_buffer: vec![
                    ProxyTest {
                        proxy: Proxy::from_str(
                            ProxyFormat::HostPortUsernamePassword,
                            "host:1234:username:password",
                        )
                        .unwrap(),
                        result: Err(ProxyTestError::UnknownError),
                    },
                    ProxyTest {
                        proxy: Proxy::from_str(
                            ProxyFormat::HostPortUsernamePassword,
                            "host:1234:username:password",
                        )
                        .unwrap(),
                        result: Ok(proxytester::ProxyTestSuccess {
                            duration: Duration::from_secs(1),
                        }),
                    },
                ],
                proxy_test_recv: tokio::sync::mpsc::channel(1).1,
            },
            selected_proxy: 0,
            exit: false,
        };

        terminal
            .draw(|frame| frame.render_widget(&app, frame.size()))
            .unwrap();

        let mut expected = Buffer::with_lines([
            "┌ProxyTester-Information───────────────────────────────────────────────────────────────────────────┐",
            "│Proxies: 10                                                                                       │",
            "│URL: https://google.com                                                                           │",
            "│Workers: 5                                                                                        │",
            "│Timeout: 5s                                                                                       │",
            "│Version: v0.1.0                                                                                   │",
            "└──────────────────────────────────────────────────────────────────────────────────────────────────┘",
            "┌Progress──────────────────────────────────────────────────────────────────────────────────────────┐",
            "│███████████████████▋                           2/10                                               │",
            "└──────────────────────────────────────────────────────────────────────────────────────────────────┘",
            "┌Test-Results──────────────────────────────────────────────────────────────────────────────────────┐",
            "│ * http://username:password@host:1234         some unknown error happened               N/A       █",
            "└──────────────────────────────────────────────────────────────────────────────────────────────────┘",
        ]);
        // Set the colors for the progress bar
        for x in 1..=98 {
            expected.get_mut(x, 8).set_fg(Color::White);
        }
        // Set the modifiers for the progress label
        for x in 48..=51 {
            expected
                .get_mut(x, 8)
                .set_style(Style::new().bold().italic().fg(Color::DarkGray));
        }
        // Set the colors for the selected row
        for x in 1..=98 {
            expected.get_mut(x, 11).set_fg(Color::DarkGray);
        }
        terminal.backend().assert_buffer(&expected);

        // Scroll down
        app.selected_proxy = 1;

        // Test the new state
        terminal
            .draw(|frame| frame.render_widget(&app, frame.size()))
            .unwrap();

        let mut expected = Buffer::with_lines([
                "┌ProxyTester-Information───────────────────────────────────────────────────────────────────────────┐",
                "│Proxies: 10                                                                                       │",
                "│URL: https://google.com                                                                           │",
                "│Workers: 5                                                                                        │",
                "│Timeout: 5s                                                                                       │",
                "│Version: v0.1.0                                                                                   │",
                "└──────────────────────────────────────────────────────────────────────────────────────────────────┘",
                "┌Progress──────────────────────────────────────────────────────────────────────────────────────────┐",
                "│███████████████████▋                           2/10                                               │",
                "└──────────────────────────────────────────────────────────────────────────────────────────────────┘",
                "┌Test-Results──────────────────────────────────────────────────────────────────────────────────────┐",
                "│ * http://username:password@host:1234         Success                                   1.000s    █",
                "└──────────────────────────────────────────────────────────────────────────────────────────────────┘",
            ]);
        // Set the colors for the progress bar
        for x in 1..=98 {
            expected.get_mut(x, 8).set_fg(Color::White);
        }
        // Set the modifiers for the progress label
        for x in 48..=51 {
            expected
                .get_mut(x, 8)
                .set_style(Style::new().bold().italic().fg(Color::DarkGray));
        }
        // Set the colors for the selected row
        for x in 1..=98 {
            expected.get_mut(x, 11).set_fg(Color::DarkGray);
        }
        terminal.backend().assert_buffer(&expected);
    }

    #[test]
    fn exiting_application_should_close_channel() {
        let recv = tokio::sync::mpsc::channel(1).1;
        let mut app = App {
            state: AppState {
                workers: 5,
                timeout: Duration::from_secs(5),
                url: "https://google.com".to_string(),
                proxy_count: 10,

                results_buffer: vec![],
                proxy_test_recv: recv,
            },
            selected_proxy: 1,
            exit: false,
        };

        app.handle_key_event(KeyEvent {
            state: KeyEventState::NONE,
            code: KeyCode::Esc,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
        });

        assert!(app.exit);
        assert!(app.state.proxy_test_recv.is_closed());
    }

    #[test]
    fn pressing_down_should_select_next_proxy() {
        let mut app = App {
            state: AppState {
                workers: 5,
                timeout: Duration::from_secs(5),
                url: "https://google.com".to_string(),
                proxy_count: 10,

                results_buffer: vec![
                    ProxyTest {
                        proxy: Proxy::from_str(
                            ProxyFormat::HostPortUsernamePassword,
                            "host:1234:username:password",
                        )
                        .unwrap(),
                        result: Err(ProxyTestError::UnknownError),
                    },
                    ProxyTest {
                        proxy: Proxy::from_str(
                            ProxyFormat::HostPortUsernamePassword,
                            "host:1234:username:password",
                        )
                        .unwrap(),
                        result: Err(ProxyTestError::UnknownError),
                    },
                ],
                proxy_test_recv: tokio::sync::mpsc::channel(1).1,
            },
            selected_proxy: 0,
            exit: false,
        };

        app.handle_key_event(KeyEvent {
            state: KeyEventState::NONE,
            code: KeyCode::Down,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
        });

        assert_eq!(app.selected_proxy, 1);
    }

    #[test]
    fn pressing_down_should_roll_back_to_start_at_end() {
        let mut app = App {
            state: AppState {
                workers: 5,
                timeout: Duration::from_secs(5),
                url: "https://google.com".to_string(),
                proxy_count: 10,

                results_buffer: vec![
                    ProxyTest {
                        proxy: Proxy::from_str(
                            ProxyFormat::HostPortUsernamePassword,
                            "host:1234:username:password",
                        )
                        .unwrap(),
                        result: Err(ProxyTestError::UnknownError),
                    },
                    ProxyTest {
                        proxy: Proxy::from_str(
                            ProxyFormat::HostPortUsernamePassword,
                            "host:1234:username:password",
                        )
                        .unwrap(),
                        result: Err(ProxyTestError::UnknownError),
                    },
                ],
                proxy_test_recv: tokio::sync::mpsc::channel(1).1,
            },
            selected_proxy: 1,
            exit: false,
        };

        app.handle_key_event(KeyEvent {
            state: KeyEventState::NONE,
            code: KeyCode::Down,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
        });

        assert_eq!(app.selected_proxy, 0);
    }

    #[test]
    fn pressing_up_should_select_previous_proxy() {
        let mut app = App {
            state: AppState {
                workers: 5,
                timeout: Duration::from_secs(5),
                url: "https://google.com".to_string(),
                proxy_count: 10,

                results_buffer: vec![
                    ProxyTest {
                        proxy: Proxy::from_str(
                            ProxyFormat::HostPortUsernamePassword,
                            "host:1234:username:password",
                        )
                        .unwrap(),
                        result: Err(ProxyTestError::UnknownError),
                    },
                    ProxyTest {
                        proxy: Proxy::from_str(
                            ProxyFormat::HostPortUsernamePassword,
                            "host:1234:username:password",
                        )
                        .unwrap(),
                        result: Err(ProxyTestError::UnknownError),
                    },
                ],
                proxy_test_recv: tokio::sync::mpsc::channel(1).1,
            },
            selected_proxy: 1,
            exit: false,
        };

        app.handle_key_event(KeyEvent {
            state: KeyEventState::NONE,
            code: KeyCode::Up,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
        });

        assert_eq!(app.selected_proxy, 0);
    }

    #[test]
    fn pressing_up_should_roll_back_to_end_at_start() {
        let mut app = App {
            state: AppState {
                workers: 5,
                timeout: Duration::from_secs(5),
                url: "https://google.com".to_string(),
                proxy_count: 10,

                results_buffer: vec![
                    ProxyTest {
                        proxy: Proxy::from_str(
                            ProxyFormat::HostPortUsernamePassword,
                            "host:1234:username:password",
                        )
                        .unwrap(),
                        result: Err(ProxyTestError::UnknownError),
                    },
                    ProxyTest {
                        proxy: Proxy::from_str(
                            ProxyFormat::HostPortUsernamePassword,
                            "host:1234:username:password",
                        )
                        .unwrap(),
                        result: Err(ProxyTestError::UnknownError),
                    },
                ],
                proxy_test_recv: tokio::sync::mpsc::channel(1).1,
            },
            selected_proxy: 0,
            exit: false,
        };

        app.handle_key_event(KeyEvent {
            state: KeyEventState::NONE,
            code: KeyCode::Up,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
        });

        assert_eq!(app.selected_proxy, 1);
    }

    #[test]
    fn pressing_random_key_should_do_nothing() {
        let mut app = App {
            state: AppState {
                workers: 5,
                timeout: Duration::from_secs(5),
                url: "https://google.com".to_string(),
                proxy_count: 10,

                results_buffer: vec![
                    ProxyTest {
                        proxy: Proxy::from_str(
                            ProxyFormat::HostPortUsernamePassword,
                            "host:1234:username:password",
                        )
                        .unwrap(),
                        result: Err(ProxyTestError::UnknownError),
                    },
                    ProxyTest {
                        proxy: Proxy::from_str(
                            ProxyFormat::HostPortUsernamePassword,
                            "host:1234:username:password",
                        )
                        .unwrap(),
                        result: Err(ProxyTestError::UnknownError),
                    },
                ],
                proxy_test_recv: tokio::sync::mpsc::channel(1).1,
            },
            selected_proxy: 0,
            exit: false,
        };

        app.handle_key_event(KeyEvent {
            state: KeyEventState::NONE,
            code: KeyCode::BackTab,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
        });

        assert_eq!(app.selected_proxy, 0);
    }
}
