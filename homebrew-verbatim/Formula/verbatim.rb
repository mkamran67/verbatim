class Verbatim < Formula
  desc "Real-time speech-to-text with push-to-talk hotkey"
  homepage "https://github.com/mkamran67/homebrew-verbatim"
  version "0.1.0"
  license "MIT"

  option "with-cuda", "Use NVIDIA CUDA GPU acceleration (Linux only)"
  option "with-vulkan", "Use Vulkan GPU acceleration for NVIDIA + AMD (Linux only)"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/mkamran67/homebrew-verbatim/releases/download/v#{version}/verbatim-#{version}-macos-arm64.tar.gz"
      sha256 "REPLACE_WITH_MACOS_ARM64_SHA256"
    else
      url "https://github.com/mkamran67/homebrew-verbatim/releases/download/v#{version}/verbatim-#{version}-macos-x86_64.tar.gz"
      sha256 "REPLACE_WITH_MACOS_X86_64_SHA256"
    end
  end

  on_linux do
    if build.with?("cuda") && build.with?("vulkan")
      odie "Pick one GPU backend: --with-cuda OR --with-vulkan, not both."
    end

    if build.with?("cuda")
      if Hardware::CPU.arm?
        url "https://github.com/mkamran67/homebrew-verbatim/releases/download/v#{version}/verbatim-#{version}-linux-arm64-cuda.tar.gz"
        sha256 "REPLACE_WITH_LINUX_ARM64_CUDA_SHA256"
      else
        url "https://github.com/mkamran67/homebrew-verbatim/releases/download/v#{version}/verbatim-#{version}-linux-amd64-cuda.tar.gz"
        sha256 "REPLACE_WITH_LINUX_AMD64_CUDA_SHA256"
      end
    elsif build.with?("vulkan")
      if Hardware::CPU.arm?
        url "https://github.com/mkamran67/homebrew-verbatim/releases/download/v#{version}/verbatim-#{version}-linux-arm64-vulkan.tar.gz"
        sha256 "REPLACE_WITH_LINUX_ARM64_VULKAN_SHA256"
      else
        url "https://github.com/mkamran67/homebrew-verbatim/releases/download/v#{version}/verbatim-#{version}-linux-amd64-vulkan.tar.gz"
        sha256 "REPLACE_WITH_LINUX_AMD64_VULKAN_SHA256"
      end
    else
      if Hardware::CPU.arm?
        url "https://github.com/mkamran67/homebrew-verbatim/releases/download/v#{version}/verbatim-#{version}-linux-arm64-cpu.tar.gz"
        sha256 "REPLACE_WITH_LINUX_ARM64_CPU_SHA256"
      else
        url "https://github.com/mkamran67/homebrew-verbatim/releases/download/v#{version}/verbatim-#{version}-linux-amd64-cpu.tar.gz"
        sha256 "REPLACE_WITH_LINUX_AMD64_CPU_SHA256"
      end
    end
  end

  def install
    bin.install "verbatim"
  end

  def caveats
    s = ""

    on_linux do
      s += <<~EOS
        You must be in the 'input' group for the global hotkey:
          sudo usermod -aG input $USER
        Then log out and back in.
      EOS

      if build.with?("cuda")
        s += <<~EOS

          CUDA build installed. NVIDIA drivers are required at runtime
          (CUDA toolkit is NOT needed — it's statically linked).
        EOS
      elsif build.with?("vulkan")
        s += <<~EOS

          Vulkan build installed. GPU drivers required at runtime:
            NVIDIA: nvidia-driver-* + libvulkan1
            AMD:    mesa-vulkan-drivers + libvulkan1
        EOS
      else
        s += <<~EOS

          CPU build installed. For GPU acceleration, reinstall with:
            brew reinstall verbatim --with-cuda    # NVIDIA
            brew reinstall verbatim --with-vulkan  # NVIDIA + AMD
        EOS
      end
    end

    on_macos do
      s += <<~EOS
        Grant Accessibility permissions for keyboard simulation:
          System Settings > Privacy & Security > Accessibility
        Grant Microphone permissions when prompted on first launch.
        Metal GPU acceleration is enabled automatically.
      EOS
    end

    s
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/verbatim --version 2>&1", 1)
  end
end
