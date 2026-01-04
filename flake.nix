{
  description = "FX - A Rust effect system with declarative abilities and type-safe capability composition";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        # Use the latest stable Rust with the 2024 edition support
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
        };

        # Common build inputs
        buildInputs = with pkgs; [
          openssl
          pkg-config
        ] ++ lib.optionals stdenv.isDarwin [
          darwin.apple_sdk.frameworks.Security
          darwin.apple_sdk.frameworks.SystemConfiguration
        ];

        nativeBuildInputs = with pkgs; [
          rustToolchain
          cargo-watch
          cargo-nextest
          cargo-audit
          cargo-deny
        ];
      in
      {
        # Development shell
        devShells.default = pkgs.mkShell {
          inherit buildInputs nativeBuildInputs;

          RUST_BACKTRACE = 1;
          RUST_LOG = "debug";

          shellHook = ''
            echo "FX development environment"
            echo "Rust version: $(rustc --version)"
            echo ""
            echo "Available commands:"
            echo "  cargo build    - Build the project"
            echo "  cargo test     - Run tests"
            echo "  cargo clippy   - Run linter"
            echo "  cargo fmt      - Format code"
            echo "  cargo watch    - Watch for changes"
          '';
        };

        # Package definition
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "fx";
          version = "0.1.0";
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          inherit buildInputs;
          nativeBuildInputs = [ pkgs.pkg-config ];

          meta = with pkgs.lib; {
            description = "A Rust effect system with declarative abilities and type-safe capability composition";
            homepage = "https://github.com/user/fx-rs";
            license = with licenses; [ mit asl20 ];
            maintainers = [ ];
          };
        };

        # Checks to run in CI
        checks = {
          # Build check
          build = self.packages.${system}.default;

          # Format check
          fmt = pkgs.runCommand "check-fmt" {
            buildInputs = [ rustToolchain ];
          } ''
            cd ${./.}
            cargo fmt --all -- --check
            touch $out
          '';

          # Clippy check
          clippy = pkgs.runCommand "check-clippy" {
            buildInputs = [ rustToolchain ] ++ buildInputs;
            nativeBuildInputs = [ pkgs.pkg-config ];
          } ''
            cd ${./.}
            cargo clippy --all-targets --all-features -- -D warnings
            touch $out
          '';

          # Test check
          test = pkgs.runCommand "check-test" {
            buildInputs = [ rustToolchain ] ++ buildInputs;
            nativeBuildInputs = [ pkgs.pkg-config ];
          } ''
            cd ${./.}
            cargo test --all
            touch $out
          '';
        };
      }
    );
}
