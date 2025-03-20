# CI/CD Workflows Documentation

This document details the continuous integration and continuous deployment workflows for the L402 Server Example project.

## CI Workflow

The CI workflow runs on every push to the main branch and on pull requests targeting main. It consists of several jobs that run in sequence:

### 1. Check

Performs initial quality checks on the codebase:

- **Code Format**: Ensures all code follows Rust formatting guidelines using `cargo fmt`.
- **Linting**: Runs Clippy to catch common mistakes and enforce code quality.
- **Compilation**: Verifies that the code compiles correctly with all features enabled.

This job sets up a Redis service container for any checks that might need database access.

### 2. Test

Runs the full test suite:

- **Unit Tests**: Tests individual components in isolation.
- **Integration Tests**: Tests interactions between components.
- **Code Coverage**: Generates coverage reports using cargo-tarpaulin.

The test job depends on the check job passing successfully. It also uses a Redis service container for tests that require database functionality.

### 3. Security Audit

Performs security checks:

- **Dependency Audit**: Scans dependencies for known vulnerabilities using cargo-audit.

### 4. Build

Builds the production artifacts:

- **Release Binary**: Compiles the application in release mode.
- **Artifact Upload**: Uploads the compiled binary as a job artifact.

This job runs only if the check and test jobs pass successfully.

### 5. Docker

Builds the Docker image:

- **Docker Build**: Creates the Docker image with the application.
- **Caching**: Uses GitHub's cache for faster builds.

This job only runs on the main branch and depends on the build job passing successfully.

## Release Workflow

The release workflow is triggered when a release is published on GitHub or manually with a specified tag.

### 1. Release Build

Creates release artifacts:

- **Binary Build**: Compiles the application in release mode.
- **Archive Creation**: Packages the binary with documentation, license, and configuration examples.
- **Asset Upload**: Uploads the archive to the GitHub release.

### 2. Docker Publish

Publishes Docker images:

- **Image Build**: Creates the Docker image with the application.
- **Registry Push**: Pushes the image to GitHub Container Registry (ghcr.io).
- **Tagging**: Tags the image with both the specific version and 'latest'.

## GitHub Repository Settings

To make the most of these workflows, ensure the repository has the following settings:

1. **Branch Protection Rules** for the main branch:
   - Require status checks to pass before merging
   - Require branches to be up-to-date before merging

2. **GitHub Packages** enabled for Docker image publishing

3. **Workflow Permissions** set to allow:
   - Write access to packages (for Docker publishing)
   - Write access to contents (for release assets)