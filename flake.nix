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

        # Runtime/build deps (migrated from devenv.nix).
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

        env = {
          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
          LD_LIBRARY_PATH = "${pkgs.mesa}/lib:${pkgs.libGL}/lib:${pkgs.wayland}/lib:${pkgs.libxkbcommon}/lib";
          __EGL_VENDOR_LIBRARY_DIRS = "/run/opengl-driver/share/glvnd/egl_vendor.d";
        };
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "gtk_learn";
          version = "0.1.0";
          src = ./.;

          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = with pkgs; [
            pkg-config
            clang
            llvmPackages.libclang
            cmake
            ninja
            python3
          ];

          buildInputs = packages;

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
              ++ packages;
          }
          // env
        );
      }
    );
}
