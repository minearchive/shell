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
  ];
}
