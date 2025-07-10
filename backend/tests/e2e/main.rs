// E2E tests entry point
// This file allows cargo test --test e2e to run all E2E tests
// E2E tests are ignored by default in CI - run with: cargo test --test e2e -- --ignored

mod upload_e2e;
mod utils;
