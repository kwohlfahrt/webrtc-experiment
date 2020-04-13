with import <nixpkgs-unstable> {}; callPackage ./server.nix {} // {
  # Environment Variables
  RUST_BACKTRACE = 1;
}
