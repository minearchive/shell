{
  description = "Quickshell configuration";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    quickshell = {
      url = "git+https://git.outfoxxed.me/outfoxxed/quickshell";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { self, nixpkgs, quickshell }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          qs = quickshell.packages.${system}.default;
        in
        {
          default = pkgs.stdenvNoCC.mkDerivation {
            pname = "shell";
            version = "0.1.0";
            src = ./.;
            installPhase = ''
              mkdir -p $out/share/quickshell/shell
              cp -r assets bar singleton shell.qml $out/share/quickshell/shell/

              mkdir -p $out/bin
              cat > $out/bin/shell <<EOF
              #!/bin/sh
              exec ${qs}/bin/quickshell -p $out/share/quickshell/shell "\$@"
              EOF
              chmod +x $out/bin/shell
            '';
          };
        }
      );

      apps = forAllSystems (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/shell";
        };
      });
    };
}
