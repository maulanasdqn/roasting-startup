{
  description = "Roasting Startup - Indonesian startup roasting website";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    crane.url = "github:ipetkov/crane";

    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, crane, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

        rustToolchain = pkgs.rust-bin.nightly.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
          targets = [ "wasm32-unknown-unknown" ];
        };

        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        src = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter = path: type:
            (pkgs.lib.hasSuffix ".scss" path) ||
            (pkgs.lib.hasSuffix ".css" path) ||
            (pkgs.lib.hasSuffix ".html" path) ||
            (pkgs.lib.hasSuffix ".js" path) ||
            (pkgs.lib.hasSuffix ".ico" path) ||
            (pkgs.lib.hasSuffix ".png" path) ||
            (pkgs.lib.hasSuffix ".svg" path) ||
            (pkgs.lib.hasSuffix ".webp" path) ||
            (pkgs.lib.hasInfix "/public/" path) ||
            (pkgs.lib.hasInfix "/style/" path) ||
            (craneLib.filterCargoSources path type);
        };

        commonArgs = {
          inherit src;
          strictDeps = true;
          pname = "roasting-startup";
          version = "0.1.0";

          nativeBuildInputs = with pkgs; [
            pkg-config
            wasm-bindgen-cli
            binaryen
            dart-sass
          ];

          buildInputs = with pkgs; [
            openssl
          ] ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isDarwin [
            libiconv
          ];

          CARGO_BUILD_TARGET = "";
        };

        cargoArtifacts = craneLib.buildDepsOnly (commonArgs // {
          cargoExtraArgs = "--workspace";
        });

        wasmArtifacts = craneLib.buildDepsOnly (commonArgs // {
          pname = "roasting-startup-wasm-deps";
          cargoExtraArgs = "-p roasting-ui --target wasm32-unknown-unknown";
          CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
          doCheck = false;
        });

        wasmBuild = craneLib.buildPackage (commonArgs // {
          pname = "roasting-startup-wasm";
          cargoArtifacts = wasmArtifacts;
          cargoExtraArgs = "-p roasting-ui --target wasm32-unknown-unknown";
          CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
          doCheck = false;

          installPhaseCommand = ''
            mkdir -p $out/pkg
            wasm-bindgen \
              --target web \
              --out-dir $out/pkg \
              --out-name roasting-startup \
              target/wasm32-unknown-unknown/release/roasting_ui.wasm

            wasm-opt -Oz -o $out/pkg/roasting-startup_bg.wasm $out/pkg/roasting-startup_bg.wasm || true
          '';
        });

        serverBinary = craneLib.buildPackage (commonArgs // {
          pname = "roasting-api";
          cargoArtifacts = cargoArtifacts;
          cargoExtraArgs = "-p roasting-api --features headless";
          doCheck = false;
        });

        # Server with local LLM (no OpenRouter dependency)
        serverBinaryLocalLlm = craneLib.buildPackage (commonArgs // {
          pname = "roasting-api-local-llm";
          cargoArtifacts = cargoArtifacts;
          cargoExtraArgs = "-p roasting-api --features local-llm,headless";
          doCheck = false;
        });

        siteAssets = pkgs.stdenv.mkDerivation {
          pname = "roasting-startup-site";
          version = "0.1.0";
          src = ./.;

          nativeBuildInputs = [ pkgs.dart-sass ];

          buildPhase = ''
            mkdir -p site/pkg
            sass style/main.scss site/pkg/main.css --style=compressed --no-source-map
          '';

          installPhase = ''
            mkdir -p $out/site/pkg
            cp -r public/* $out/site/ 2>/dev/null || true
            cp site/pkg/main.css $out/site/pkg/
          '';
        };

        roastingStartup = pkgs.stdenv.mkDerivation {
          pname = "roasting-startup";
          version = "0.1.0";

          dontUnpack = true;
          dontBuild = true;

          installPhase = ''
            mkdir -p $out/bin $out/site/pkg

            cp ${serverBinary}/bin/roasting-api $out/bin/

            cp -r ${siteAssets}/site/* $out/site/

            cp ${wasmBuild}/pkg/* $out/site/pkg/
          '';
        };

        # Local LLM variant (uses Qwen2.5-0.5B-Instruct, no API key needed)
        roastingStartupLocalLlm = pkgs.stdenv.mkDerivation {
          pname = "roasting-startup-local-llm";
          version = "0.1.0";

          dontUnpack = true;
          dontBuild = true;

          installPhase = ''
            mkdir -p $out/bin $out/site/pkg

            cp ${serverBinaryLocalLlm}/bin/roasting-api $out/bin/

            cp -r ${siteAssets}/site/* $out/site/

            cp ${wasmBuild}/pkg/* $out/site/pkg/
          '';
        };

      in {
        packages = {
          default = roastingStartup;
          local-llm = roastingStartupLocalLlm;
          server = serverBinary;
          server-local-llm = serverBinaryLocalLlm;
          wasm = wasmBuild;
          site = siteAssets;
        };

        devShells.default = craneLib.devShell {
          checks = { };

          packages = with pkgs; [
            cargo-leptos
            dart-sass
            wasm-bindgen-cli
            binaryen
          ] ++ pkgs.lib.optionals pkgs.stdenv.isLinux [
            chromium
          ];

          LEPTOS_SITE_ROOT = "target/site";
          LEPTOS_SITE_PKG_DIR = "pkg";
          LEPTOS_SITE_ADDR = "127.0.0.1:3000";
          LEPTOS_RELOAD_PORT = "3001";
        };
      }
    ) // {
      nixosModules.default = import ./nix/module.nix self;
    };
}
