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
