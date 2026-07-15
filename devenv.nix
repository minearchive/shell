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
    libepoxy
    mesa
    libGL
    wayland
    libxkbcommon
  ];

  env.LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
  env.LD_LIBRARY_PATH = "${pkgs.mesa.drivers}/lib:${pkgs.libGL}/lib:${pkgs.wayland}/lib:${pkgs.libxkbcommon}/lib";
  env.__EGL_VENDOR_LIBRARY_DIRS = "/run/opengl-driver/share/glvnd/egl_vendor.d";
}
