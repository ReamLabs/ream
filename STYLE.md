# Ream Style Guide

This document explains the code style rules and practices **all contributors and users** must follow for this repository. Our CI pipeline strictly enforces these rules. Following them will ensure smooth PR reviews, fast CI, and a maintainable codebase.


## 1. Formatting with rustfmt

**All code must be formatted using [rustfmt](https://github.com/rust-lang/rustfmt) with our workspace’s `rustfmt.toml`.**  
Formatting is **strictly enforced by CI**.

- **How to format your code:**
  ```sh
  cargo +nightly fmt
  ```
- To **check** adherence (what CI does), run:
  ```sh
  cargo +nightly fmt -- --check
  ```

If your pull request includes unformatted code, it **will NOT be merged**.


## 2. Naming Rule for `.map_err` Closures

### What is enforced?

When using `.map_err` to map errors, **the closure argument must be named `err`** (not `e`, `error`, etc):

```rust
// ✅ Correct:
.map_err(|err| { ... })
```
```rust
// ❌ Incorrect:
.map_err(|e| { ... })
.map_err(|error| { ... })
```

### **Why?**

This ensures consistency and makes tracing error handling reliable for all contributors.

## 3. Imports: Grouping & Sorting

We use [cargo-sort](https://github.com/DevinR528/cargo-sort) to enforce import grouping and sorting.

- **How to check locally:**
  ```sh
  cargo sort --grouped --check --workspace
  ```
- **How to fix:**
  ```sh
  cargo sort --grouped --workspace
  ```

Failing to comply will cause CI to fail.  
Groups: std, external crates, and internal (crate) imports; all alphabetically sorted.

## 4. Linting with Clippy

All code must pass [Clippy](https://github.com/rust-lang/rust-clippy) with **no warnings**.

- CI runs:
  ```sh
  cargo clippy --all --all-targets --no-deps -- --deny warnings
  cargo clippy --package ream-bls --all-targets --features "supranational" --no-deps -- --deny warnings
  ```

## 5. Pull Request Requirements for Contributors

To avoid CI failures:
- Always **format** your code.
- Use `err` as the closure argument for all `.map_err` usages.
- Group and sort imports.
- Pass Clippy and other CI tests.

**Summary Table for Local Checks:**

| Check               | Command                                                            |
|---------------------|--------------------------------------------------------------------|
| Format code         | `cargo +nightly fmt`                                               |
| Check formatting    | `cargo +nightly fmt -- --check`                                    |
| Sort/check imports  | `cargo sort --grouped --check --workspace`                         |
| Clippy              | `cargo clippy --all --all-targets --no-deps -- --deny warnings`    |
| Run tests           | `cargo test --release --workspace -- --nocapture`                  |

If you have questions, see [CONTRIBUTING.md] or discuss with the maintainers.

_This style is maintained automatically and enforced in CI. Submissions not following these rules will be rejected by the automated checks._


