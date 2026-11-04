# irgendwas: Secret Santa

A web application for organizing Secret Santa gift exchanges: create groups, invite
participants, keep wishlists and let the app draw who gives to whom, with exclusion rules
and e-mail notifications (English and German).

The app is written in Rust with Leptos (SSR + hydration), Axum and MariaDB. Everything
lives in [`rust/`](rust/README.md): development setup, tests, configuration, production
deployment and the migration of the old PHP data.

- `rust/`: the application (cargo-leptos workspace)
- `docs/rewrite/`: plan, decision log and specs of the rewrite
- `.forgejo/workflows/rust.yml`: CI (checks, end-to-end tests, container image)

The previous PHP version is kept on the branch `dev_php`.

## License

MIT, see [LICENSE](LICENSE).
