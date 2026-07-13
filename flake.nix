{
  description = "hyprPanopticon - Hyprland workspace overview on a circle";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      forSystems = f: nixpkgs.lib.genAttrs [ "x86_64-linux" "aarch64-linux" ]
        (system: f nixpkgs.legacyPackages.${system});
    in
    {
      devShells = forSystems (pkgs: {
        default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            rustc
            cargo
            rustfmt
            clippy
            rust-analyzer
            pkg-config
          ];
          buildInputs = with pkgs; [
            gtk4
            gtk4-layer-shell
            glib
            wayland
            libxkbcommon
          ];
        };
      });

      packages = forSystems (pkgs: {
        default = pkgs.rustPlatform.buildRustPackage {
          pname = "hyprpanopticon";
          version = "0.1.0";
          src = self;
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = with pkgs; [ pkg-config wrapGAppsHook4 ];
          buildInputs = with pkgs; [ gtk4 gtk4-layer-shell ];
          meta = {
            description = "Hyprland workspace overview on a circle";
            mainProgram = "hyprpanopticon";
          };
        };
      });
    };
}
