{
  description = "Wayland layer-shell bar rendered with Skia";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};

        # Shared native build and runtime dependencies.
        packages = with pkgs; [
          gtk3
          gtk4
          libadwaita
          gtk4-layer-shell
          gcc
          clang
          llvmPackages.libclang
          python3
          pkg-config
          gnumake
          cmake
          ninja
          libepoxy
          mesa
          libGL
          wayland
          libxkbcommon
        ];

        # Programs the bar runs: curl fetches players' remote cover art.
        runtimePackages = with pkgs; [
          curl
        ];

        env = {
          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
            pkgs.mesa
            pkgs.libGL
            pkgs.wayland
            pkgs.libxkbcommon
          ];
          __EGL_VENDOR_LIBRARY_DIRS = "/run/opengl-driver/share/glvnd/egl_vendor.d";
        };
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "gtk_learn";
          version = "0.1.0";
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
            outputHashes = {
              "hyprland-0.4.0-beta.3" = "sha256-uhCZylXf81fvlBd6ToIOrlr3InVXLzv25LyvxxpmLWo=";
            };
          };

          nativeBuildInputs = with pkgs; [
            pkg-config
            clang
            llvmPackages.libclang
            cmake
            ninja
            python3
            makeWrapper
          ];

          buildInputs = packages;

          # The sandbox has no network for skia-bindings to fetch this itself.
          # ponytail: x86_64-linux only, and tied to skia-safe's version and features: the
          # key is in target/debug/build/skia-bindings-*/out/skia/key.txt after an update.
          SKIA_BINARIES_URL = "file://${
            pkgs.fetchurl {
              url = "https://github.com/rust-skia/skia-binaries/releases/download/0.97.2/skia-binaries-da8fc6731fc439bc3b6a-x86_64-unknown-linux-gnu-jpegd-jpege-pdf.tar.gz";
              hash = "sha256-IXTSyAg9Jv2STUtR6rKBzdWYkHK0dDZmenEVB4j2/js=";
            }
          }";

          # ninja is here for skia's build, not ours: keep its hook off cargo's phases.
          dontUseNinjaBuild = true;
          dontUseNinjaInstall = true;
          dontUseNinjaCheck = true;

          # They need system fonts and a session bus, which the sandbox has neither of. Run
          # them in the dev shell.
          doCheck = false;

          postFixup = ''
            wrapProgram $out/bin/my_shell \
              --suffix PATH : ${pkgs.lib.makeBinPath runtimePackages}
          '';

          inherit (env) LIBCLANG_PATH;
        };

        devShells.default = pkgs.mkShell (
          {
            buildInputs =
              (with pkgs; [
                rustc
                cargo
                clippy
                rustfmt
                rust-analyzer
              ])
              ++ packages
              ++ runtimePackages;
          }
          // env
        );
      }
    );
}
