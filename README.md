# Proxy Tester

This repository contains a proxy tester tool that allows you to test the functionality and performance of different proxy servers.

Note: This is a tool made for personal use, therefore features won't exactly align with what you might require.

## Installation

### Prerequisites:

- [rust](https://www.rust-lang.org/)
- [libcurl](https://curl.se/download.html) (This will be preinstalled by default on lots of OS's. Most likely you can ignore this prerequisite. Otherwise, installing cURL will do the trick.)

To install the proxy tester, there are several options:

### ArchLinux

Arch users may download ProxyTester from the [AUR](https://aur.archlinux.org/packages/proxytester).

### Cargo (crates.io)

You can install the latest published version on Crates.io by using:

```bash
$ cargo install proxytester
```

### Cargo (Github source)

You can install the latest published version on Crates.io by using:

```bash
$ cargo install --git https://github.com/einstein8612/proxytester.git --tag v0.1.0
```

## Usage

To see how to use the proxytester, you can view the help menu.

```bash
$ proxytester --help

Usage: proxytester.exe [OPTIONS] <FILES>...

Arguments:
  <FILES>...  File to read the proxies from

Options:
  -u, --url <URL>             The URL to test the proxies against [default: https://1.1.1.1]
  -w, --workers <WORKERS>     How many workers to use, ergo how many proxies to test at once [default: 1]
  -t, --timeout <TIMEOUT_MS>  Timeout for each request in milliseconds [default: 5000]
  -h, --help                  Print help
  -V, --version               Print version
```

### Examples

```bash
$ proxytester --url="http://1.1.1.1" proxies.txt
```

[//]: # "TODO: Add images here"

```bash
$ proxytester --workers=5 --url="http://1.1.1.1" proxies.txt
```

[//]: # "TODO: Add images here"

## Lib Usage

You can also use the ProxyTester as a library, and it was mainly built for this purpose.

```rust
let mut proxy_tester = ProxyTesterOptions::default()
    .set_url("http://1.1.1.1".to_owned())
    .set_workers(5)
    .set_timeout(Duration::from_millis(5000))
    .build();

let recv: Receiver<ProxyTest> = proxy_tester.run().await;

// You use the recv channel to read all results as they come in.
// ...
```

## Contributing

Contributions are welcome, please open an issue or submit a pull request.

## License

This project is licensed under the [MIT License](LICENSE).
