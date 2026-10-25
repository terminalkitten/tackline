class Tackline < Formula
  desc "Point at code, write what your coding agent should do there, and hand it one Markdown prompt. Works with Claude Code, Codex, opencode, aider, Cursor and any agent that takes a prompt."
  homepage "https://terminalkitten.github.io/tackline/"
  version "0.1.2"
  if OS.mac?
    if Hardware::CPU.arm?
      url "https://github.com/terminalkitten/tackline/releases/download/v0.1.2/tackline-aarch64-apple-darwin.tar.xz"
      sha256 "2bf825222c07cc2dc9fc9470e9a9487decf246ed4e4b88b26140f9155cd3ca1f"
    end
    if Hardware::CPU.intel?
      url "https://github.com/terminalkitten/tackline/releases/download/v0.1.2/tackline-x86_64-apple-darwin.tar.xz"
      sha256 "b713cbfd93cb72598470cccb93f75076506ff036b985ab1a82ed6d6d1b9ece39"
    end
  end
  if OS.linux?
    if Hardware::CPU.arm?
      url "https://github.com/terminalkitten/tackline/releases/download/v0.1.2/tackline-aarch64-unknown-linux-gnu.tar.xz"
      sha256 "078d520c00cf491911bd0f201dd8159d0fe4f264b4b161ddf1c573170b411103"
    end
    if Hardware::CPU.intel?
      url "https://github.com/terminalkitten/tackline/releases/download/v0.1.2/tackline-x86_64-unknown-linux-gnu.tar.xz"
      sha256 "2debb8c474684d7bcdcbaa9e58b6c0a0086df298eff46d1f3f587494cfc392a1"
    end
  end
  license "MIT"

  BINARY_ALIASES = {
    "aarch64-apple-darwin": {},
    "aarch64-unknown-linux-gnu": {},
    "x86_64-apple-darwin": {},
    "x86_64-pc-windows-gnu": {},
    "x86_64-unknown-linux-gnu": {}
  }

  def target_triple
    cpu = Hardware::CPU.arm? ? "aarch64" : "x86_64"
    os = OS.mac? ? "apple-darwin" : "unknown-linux-gnu"

    "#{cpu}-#{os}"
  end

  def install_binary_aliases!
    BINARY_ALIASES[target_triple.to_sym].each do |source, dests|
      dests.each do |dest|
        bin.install_symlink bin/source.to_s => dest
      end
    end
  end

  def install
    if OS.mac? && Hardware::CPU.arm?
      bin.install "tack", "tackline"
    end
    if OS.mac? && Hardware::CPU.intel?
      bin.install "tack", "tackline"
    end
    if OS.linux? && Hardware::CPU.arm?
      bin.install "tack", "tackline"
    end
    if OS.linux? && Hardware::CPU.intel?
      bin.install "tack", "tackline"
    end

    install_binary_aliases!

    # Homebrew will automatically install these, so we don't need to do that
    doc_files = Dir["README.*", "readme.*", "LICENSE", "LICENSE.*", "CHANGELOG.*"]
    leftover_contents = Dir["*"] - doc_files

    # Install any leftover files in pkgshare; these are probably config or
    # sample files.
    pkgshare.install(*leftover_contents) unless leftover_contents.empty?
  end
end
