{
  description = "Qubitick Corpus Compiler";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        rustToolchain = pkgs.rust-bin.stable.latest.default;
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain
            pkgs.sqlite
            pkgs.python3
            pkgs.typst
            pkgs.lean4
            pkgs.pkg-config
            pkgs.tectonic   # pdf invokes the `tectonic` binary as a subprocess
          ];

          shellHook = ''
            echo "Welcome to the Qubitick Corpus Environment."
            echo "Rust: $(rustc --version)"
            echo "SQLite: $(sqlite3 --version | cut -d' ' -f1)"
            echo "Typst: $(typst --version)"
            echo "Lean: $(lean --version)"
            echo "Tectonic: $(which tectonic)"
          '';
        };
      });
}
