{
  inputs = {
    naersk.url = "github:nix-community/naersk/master";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      utils,
      naersk,
    }:
    utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
        naersk-lib = pkgs.callPackage naersk { };
      in
      {
        packages.default = naersk-lib.buildPackage {
          src = ./.;
          nativeBuildInputs = with pkgs; [
            installShellFiles
            shared-mime-info
          ];
          buildInputs = with pkgs; [ libiconv ];

          precheck = ''
            export HOME=$TEMPDIR
          '';

          postInstall = ''
            installShellCompletion --cmd handlr \
              --zsh <(COMPLETE=zsh $out/bin/handlr) \
              --bash <(COMPLETE=bash $out/bin/handlr) \
              --fish <(COMPLETE=fish $out/bin/handlr) \

            installManPage target/release/build/handlr-regex-*/out/manual/man1/*
          '';
        };
        devShell =
          with pkgs;
          mkShell {
            buildInputs = [
              cargo
              cargo-insta # Adds `cargo insta` command for reviewing snapshots
              cargo-mutants # Adds `cargo mutants` command for mutation testing
              clippy # Adds more checks to cargo
              pre-commit
              rust-analyzer # Rust language server
              rustc
              rustfmt
            ];
            RUST_SRC_PATH = rustPlatform.rustLibSrc;
          };
      }
    );
}
