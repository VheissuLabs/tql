# The formula published to VheissuLabs/homebrew-tap on release.
# VERSION and SHA are filled in by the release workflow.
class Tql < Formula
  desc "Database client for the terminal"
  homepage "https://github.com/VheissuLabs/tql"
  url "https://github.com/VheissuLabs/tql/releases/download/vVERSION/tql"
  version "VERSION"
  sha256 "SHA"
  license "MIT"

  depends_on "php" => "8.4"

  def install
    bin.install "tql"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/tql --version")
  end
end
