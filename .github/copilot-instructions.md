# GitHub Copilot Instructions for Rust Development

**Scope**: These guidelines apply to ALL Rust files (*.rs) in this repository, including:
- `src/` - Main source code
- `bin/` - Binary crates
- `crates/` - Workspace crates
- `examples/` - Example code
- `tests/` - Integration tests
- `benches/` - Benchmarks

## Ream Code Standards & Expectations

### Variable Naming and Declaration
- **Lazy declaration**: Only define variables when you need them, not at the beginning of functions
- **Meaningful names**: Variable names should clearly describe their purpose and content
- **Snake_case**: Use snake_case for variables, functions, and modules
- **PascalCase**: Use PascalCase for types, structs, enums, and traits

### Logging and Output Best Practices
- **Inline variables in macros**: Inline variables directly in `info!`, `debug!`, `warn!`, `error!`,`println!` and similar macros wherever possible.
  ```rust
  // ✅ Good
  info!("Processing user {user_id} with status {status}");
  
  // ❌ Bad
  info!("Processing user {} with status {}", user_id, status);
  ```
- **Use structured logging**: Include context and relevant data in log messages
- **Appropriate log levels**: Use the correct log level for different types of messages

### Error Handling
- **Prefer anyhow::Result`**: Prefer using anyhow::Result instead of Result<T, E>.
- **Propagate errors**: Use `?` operator to propagate errors up the call stack
- **Meaningful error messages**: Provide context about what operation failed and why
- **Avoid unwrap()**: Only use `unwrap()` when you can prove the operation cannot fail

### Memory Management and Performance
- **Prefer borrowing**: Use references (`&T`) instead of owned values when possible
- **Avoid unnecessary clones**: Only clone when ownership transfer is required
- **Use `Cow<str>`** when you might need either borrowed or owned strings
- **Prefer iterators**: Use iterator chains over manual loops when appropriate
- **Avoid premature optimization**: Write clear code first, optimize when needed

### Type Safety and Design
- **Strong typing**: Use newtype patterns for domain-specific types
- **Prefer enums**: Use enums with variants instead of boolean flags or magic numbers
- **Implement standard traits**: Derive or implement `Debug`, `Clone`, `PartialEq` as appropriate
- **Use type-level constants**: Prefer `const` over hardcoded values
- **Validate at boundaries**: Validate input at API boundaries, trust internal data

### Function and Module Design
- **Small functions**: Keep functions focused on a single responsibility
- **Pure functions**: Prefer functions without side effects when possible
- **Clear signatures**: Function signatures should be self-documenting
- **Module organization**: Group related functionality in modules
- **Public API**: Minimize public surface area, prefer private by default

### Testing
- **Unit tests**: Write tests for individual functions and methods
- **Integration tests**: Test module interactions and public APIs
- **Property-based testing**: Use `proptest` for complex invariants
- **Test naming**: Use descriptive test names that explain the scenario
- **Arrange-Act-Assert**: Structure tests with clear setup, execution, and verification

### Documentation
- **Doc comments**: Use `///` for public APIs with examples
- **Module docs**: Document module purpose and usage patterns
- **Examples**: Include code examples in documentation
- **README**: Keep README up-to-date with build and usage instructions

### Async Programming
- **Use `tokio`**: Prefer tokio ecosystem for async runtime and utilities
- **Avoid blocking**: Never use blocking operations in async contexts
- **Structured concurrency**: Use `tokio::select!` and `join!` for concurrent operations
- **Timeout operations**: Add timeouts to network and I/O operations

### Dependencies and Cargo
- **Minimal dependencies**: Only add dependencies you actually need
- **Version pinning**: Use specific versions for production applications
- **Feature flags**: Use cargo features to make dependencies optional
- **Workspace organization**: Only add a crate dependency to the workspace’s main Cargo.toml if it is required by more than one crate.

### Security Best Practices
- **Input validation**: Validate all external input
- **Secure defaults**: Choose secure defaults for configuration
- **Avoid `unsafe`**: Only use unsafe code when absolutely necessary with proper documentation
- **Dependency auditing**: Regularly audit dependencies for security vulnerabilities
- **Secret management**: Never hardcode secrets, use environment variables or secret management

### Code Organization Patterns
- **Builder pattern**: Use for complex object construction
- **RAII**: Leverage Rust's ownership system for resource management
- **Composition over inheritance**: Prefer composition and traits over complex hierarchies
- **Hexagonal architecture**: Separate business logic from external dependencies

### Specific Project Guidelines
- **Post-quantum cryptography**: Always use quantum-resistant algorithms
- **Keystore security**: Validate hex strings and cryptographic parameters
- **Account management**: Use type-safe enums for message types and crypto functions
- **Error context**: Provide meaningful context in error messages for debugging
- **Configuration**: Use strongly-typed configuration with validation

## Code Review Checklist
- [ ] Variable names are descriptive and use snake_case
- [ ] No unnecessary variable declarations
- [ ] Error handling uses Result types appropriately
- [ ] Log messages use inline variable syntax
- [ ] Functions are focused and well-named
- [ ] Tests cover the happy path and error cases
- [ ] Documentation is clear and includes examples
- [ ] No hardcoded values or magic numbers
- [ ] Memory usage is efficient (minimal cloning)
- [ ] Security considerations are addressed

## Performance Considerations
- **Profile before optimizing**: Use `cargo flamegraph` or similar tools
- **Benchmark critical paths**: Use `criterion` for performance testing
- **Memory profiling**: Monitor memory usage in long-running applications
- **Compile-time optimization**: Use const evaluation where possible
- **Zero-cost abstractions**: Leverage Rust's zero-cost abstractions

## Tooling Integration
- **Clippy**: Always run `cargo clippy` and address warnings
- **Rustfmt**: Use `cargo fmt` for consistent code formatting
- **Rust analyzer**: Configure IDE with rust-analyzer for better development experience
- **CI/CD**: Automate testing, linting, and security audits

Remember: Write code that is readable, maintainable, and follows Rust idioms. When in doubt, favor explicitness over cleverness.
