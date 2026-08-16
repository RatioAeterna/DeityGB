{
  description = "DeityGB Game Boy and Game Boy Color emulator";

  inputs = {
    nixpkgs.url = "nixpkgs/nixos-unstable"; # Adjusted to use nixos-unstable for simplicity
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils, ... }: 
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          # Removed cargo2nix overlay to simplify
        };
      in
      {
        devShell = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustc
            cargo
            rustup
            rustfmt
          ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
            pkgs.libiconv
          ] ++ pkgs.lib.optionals pkgs.stdenv.isLinux [
            pkgs.alsa-lib
            pkgs.libGL
            pkgs.xorg.libX11
            pkgs.xorg.libXi
          ];
          nativeBuildInputs = [ pkgs.pkg-config ]
            ++ pkgs.lib.optionals pkgs.stdenv.isDarwin (with pkgs.darwin.apple_sdk.frameworks; [
              Foundation
              AppKit
              CoreGraphics
              Metal
              MetalKit
              ImageIO
              Vision
              AVFoundation
            ]);
        };
      }
    );
}
