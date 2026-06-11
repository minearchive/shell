{
  pkgs,
  ...
}:

{
  languages.rust.enable = true;

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
  ];

  env.LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
}
