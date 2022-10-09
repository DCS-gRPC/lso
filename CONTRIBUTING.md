# Contributing Guide

Please [report an issue](https://github.com/DCS-gRPC/lso/issues/new?assignees=&labels=&template=1_wrong_cable.yml) if the LSO did not detect the correct cable.

## Development

To compile and run the code, execute:

```bash
cargo run -- run
```

In development, it is useful to include verbose logging. Debug logs can be activated by adding `-v`, and trace logs by adding `-vv`, e.g.:

```bash
cargo run -- run -vv
```

If you have a recording you want to replay to test changes, you can run:

```bash
cargo run --file .\RECORDING_NAME.zip.acmi
```

To run tests, execute:

```bash
cargo test
```
