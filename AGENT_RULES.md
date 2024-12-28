# Fossil Headers DB - Coding Conventions

## Agent Behaviour
- When asked to add documentation to the code, do not make any changes to the code logic itself and just decorate methods with standard documentation practices.
- Regularly suggest updates to @AGENT_RULES.md if appropriate.
- Behave like an experienced senior engineer who is used to covering all the corner cases and edge cases in the first pass.
- Behave like an architect that skillfully balances between the cost of the solution and the benefits it provides.
- Behave like a product manager that is used to working with stakeholders and understanding their needs.
- Behave like a senior engineer that is used to working with complex systems and understanding the tradeoffs between different solutions.
- For any new code written always make sure that tests are written for it.

## Rust Code Style

### General
- Follow the official Rust style guide
- Use `rustfmt` for consistent code formatting
- Maximum line length: 100 characters
- Use meaningful and descriptive variable names

### Naming Conventions
- Use snake_case for functions, variables, and modules
- Use PascalCase for types, traits, and enums
- Use SCREAMING_SNAKE_CASE for constants
- Prefix unsafe functions with `unsafe_`

### Error Handling
- Use `Result` types for operations that can fail
- Implement custom error types using `thiserror`
- Avoid unwrap() except in tests or where panic is intended
- Use anyhow::Result for application-level error handling

### Documentation
- Document all public APIs using rustdoc
- Include examples in documentation where appropriate
- Document complex algorithms and business logic
- Use `//` for single-line comments and `///` for doc comments

### Testing
- Write unit tests for all public functions
- Place tests in a `tests` module at the bottom of each file
- Use meaningful test names: `test_<function_name>_<scenario>`
- Use integration tests for database operations

## Database Conventions

### PostgreSQL
- Use lowercase for table and column names
- Use snake_case for naming
- Include appropriate indexes on frequently queried columns
- Document complex queries with comments
- Use migrations for schema changes
- Version control all database migrations

### SQL Style
- Capitalize SQL keywords
- One statement per line
- Use meaningful table aliases
- Include appropriate WHERE clauses for performance
- Use prepared statements to prevent SQL injection

### Database Operations
- Use connection pooling
- Implement proper error handling for database operations
- Use transactions where appropriate
- Include appropriate logging for database operations

## Project Structure
```
src/
  ├── db/           # Database related code
  ├── models/       # Data structures and types
  ├── handlers/     # Request handlers
  ├── utils/        # Utility functions
  └── main.rs       # Application entry point
```

## Cargo Dependencies
- Keep dependencies up to date
- Pin dependency versions in Cargo.toml
- Document why each dependency is needed
- Minimize dependency count where possible

## Version Control
- Write meaningful commit messages
- Use feature branches for development
- Review code before merging
- Keep commits focused and atomic

## Logging and Error Handling
- Use appropriate log levels (error, warn, info, debug)
- Include context in error messages
- Log all database operations
- Include timestamps in logs

## Performance Considerations
- Use async/await where appropriate
- Implement proper database indexing
- Cache frequently accessed data
- Monitor query performance

## Security
- Never commit sensitive data
- Use environment variables for configuration
- Implement proper authentication
- Sanitize all user inputs