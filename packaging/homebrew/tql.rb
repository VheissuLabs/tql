class Tql < Formula
  desc "Database client for the terminal"
  homepage "https://github.com/VheissuLabs/tql"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/VheissuLabs/tql/releases/download/vVERSION/tql-macos-aarch64"
      sha256 "SHA_MACOS_ARM"
    end
    on_intel do
      url "https://github.com/VheissuLabs/tql/releases/download/vVERSION/tql-macos-x86_64"
      sha256 "SHA_MACOS_INTEL"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/VheissuLabs/tql/releases/download/vVERSION/tql-linux-aarch64"
      sha256 "SHA_LINUX_ARM"
    end
    on_intel do
      url "https://github.com/VheissuLabs/tql/releases/download/vVERSION/tql-linux-x86_64"
      sha256 "SHA_LINUX_INTEL"
    end
  end

  def install
    bin.install Dir["tql-*"].first => "tql"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/tql --version")
  end
end
