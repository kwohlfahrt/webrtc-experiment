with import <nixpkgs> {}; callPackage ./server.nix {} // {
  # Environment Variables
  RUST_BACKTRACE = 1;
}
