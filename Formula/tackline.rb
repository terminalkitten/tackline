class Tackline < Formula
  desc "Point at code, write what your coding agent should do there, and hand it one Markdown prompt. Works with Claude Code, Codex, opencode, aider, Cursor and any agent that takes a prompt."
  homepage "https://terminalkitten.github.io/tackline/"
  version "0.1.3"
  if OS.mac?
    if Hardware::CPU.arm?
      url "https://github.com/terminalkitten/tackline/releases/download/v0.1.3/tackline-aarch64-apple-darwin.tar.xz"
      sha256 "8ddeafd6885bedeab50f4fdcdb78447d8f4b1b28f8c7b3597b920fe5fbd4967b"
    end
    if Hardware::CPU.intel?
      url "https://github.com/terminalkitten/tackline/releases/download/v0.1.3/tackline-x86_64-apple-darwin.tar.xz"
      sha256 "e6f1e43baf244e6d6955ad796c504abaa5fddafc49a1101ead9715317fc8e7d4"
    end
  end
  if OS.linux?
    if Hardware::CPU.arm?
      url "https://github.com/terminalkitten/tackline/releases/download/v0.1.3/tackline-aarch64-unknown-linux-gnu.tar.xz"
      sha256 "799486171ac76293207d1f682bbb2ad3f076766ee566e8eea656eec2ada9895e"
    end
    if Hardware::CPU.intel?
      url "https://github.com/terminalkitten/tackline/releases/download/v0.1.3/tackline-x86_64-unknown-linux-gnu.tar.xz"
      sha256 "46ba50dbea925c9f30a39e1974fc1b69ef039e77ed33ebb2bb7572b997e0643c"
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
