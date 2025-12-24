{
  description = "lean.hx";

  inputs = {
    nixpkgs.url = "nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    naersk = {
      url = "github:nix-community/naersk";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, naersk }: {
    overlays = rec {
      expects-naersk = final: prev: {
        nodePackages = prev.nodePackages.extend (_: _: {
          inherit (final.callPackage ./pkgs/development/node-packages { })
            "@leanprover/infoview";
        });

        "lean.hx" = final.callPackage
          ({ naersk }: naersk.buildPackage {
            pname = "lean.hx";
            root = builtins.path { path = ./..; name = "lean.hx-src"; };
            copyBins = false;
            copyLibs = true;
            INFOVIEW_DIST =
              "${final.nodePackages."@leanprover/infoview"}/lib/node_modules/@leanprover/infoview/dist";
          })
          { };
      };

      default = final: prev:
        let
          appended = final.appendOverlays
            [ naersk.overlays.default expects-naersk ];
        in
        {
          inherit (appended) "lean.hx";
          nodePackages = prev.nodePackages.extend
            (_: _: { inherit (appended.nodePackages) "@leanprover/infoview"; });
        };
    };
  } // flake-utils.lib.eachDefaultSystem (system:
    let
      pkgs = import nixpkgs {
        overlays = [ self.overlays.default ];
        inherit system;
      };
      inherit (pkgs) clippy mkShell node2nix nodejs nodePackages steel
        rust-analyzer rustc rustfmt rustPlatform typescript;
      inherit (nodePackages) prettier typescript-language-server;
    in
    {
      devShells.default = mkShell {
        inputsFrom = [ pkgs."lean.hx" ];
        packages = [
          clippy
          node2nix
          nodejs
          prettier
          rust-analyzer
          rustc
          rustfmt
          steel
          typescript
          typescript-language-server
        ];
        RUST_SRC_PATH = rustPlatform.rustLibSrc;
      };

      packages = {
        inherit (nodePackages) "@leanprover/infoview";
        inherit (pkgs) "lean.hx";
      };
    });
}
