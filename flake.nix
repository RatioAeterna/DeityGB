{
  description = "Robust Implementation of the LaBRADOR Proof System";

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
        rustPackageSet = pkgs.rustPlatform.rust {
          # Using rustPlatform to simplify Rust package management
          packageFun = _: {}; # Placeholder, adjust as necessary
        };
      in
      {
        devShell = pkgs.mkShell {
          buildInputs = with pkgs; [
	    rustc
	    cargo
            rustup
	    rustfmt
	    alsaLib
	    xorg.libX11
	    xorg.libXcursor
	    xorg.libXrandr
	    xorg.libXi
            # Add other essential Rust tools as needed
          ];
        };
      }
    );
}

