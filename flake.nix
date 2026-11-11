{
  description = "Wayland layer-shell bar built on AntiWebUI";

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

        # Native libraries the build links: libdbus for mpris, libwayland for wgpu, fontconfig
        # for AntiWebUI's font lookup, libxcb for its X11 backend (built in even though the bar
        # only runs on Wayland).
        packages = with pkgs; [
          pkg-config
          dbus
          fontconfig
          wayland
          libxkbcommon
          libxcb
        ];

        # What wgpu and AntiWebUI load at run time: Vulkan (or GL as a fallback), and
        # libpipewire for the volume.
        libraryPath = pkgs.lib.makeLibraryPath (
          with pkgs;
          [
            vulkan-loader
            mesa
            libGL
            wayland
            libxkbcommon
            pipewire
          ]
        );

        # Programs the bar runs: curl fetches players' remote cover art, gammastep is the
        # night light.
        runtimePackages = with pkgs; [
          curl
          gammastep
        ];

        env = {
          LD_LIBRARY_PATH = libraryPath;
          __EGL_VENDOR_LIBRARY_DIRS = "/run/opengl-driver/share/glvnd/egl_vendor.d";
        };
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "my_shell";
          version = "0.1.0";
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
            outputHashes = {
              "hyprland-0.4.0-beta.3" = "sha256-uhCZylXf81fvlBd6ToIOrlr3InVXLzv25LyvxxpmLWo=";
              "anti-webui-0.1.0" = "sha256-Dl/rMo1HAFRbSr26GvxPg0oQ00dqV2LnW1eZ12nw2Yc=";
            };
          };

          nativeBuildInputs = with pkgs; [
            pkg-config
            makeWrapper
          ];

          buildInputs = packages;

          # They need system fonts and a session bus, which the sandbox has neither of. Run
          # them in the dev shell.
          doCheck = false;

          postFixup = ''
            wrapProgram $out/bin/my_shell \
              --suffix PATH : ${pkgs.lib.makeBinPath runtimePackages} \
              --suffix LD_LIBRARY_PATH : ${libraryPath}
          '';
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
