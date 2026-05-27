"""Download the minimal OpenAI tokenizer resources used by vulcan-token-counter.
下载 vulcan-token-counter 使用的最小 OpenAI tokenizer 资源。
"""

from __future__ import annotations

import argparse
import hashlib
import subprocess
import urllib.request
from dataclasses import dataclass
from pathlib import Path


# OpenAI resources required by the lightweight budget proxy runtime.
# 轻量预算代理运行时需要的 OpenAI 资源。
OPENAI_TOKENIZER_URLS = {
    "cl100k_base.tiktoken": "https://openaipublic.blob.core.windows.net/encodings/cl100k_base.tiktoken",
    "o200k_base.tiktoken": "https://openaipublic.blob.core.windows.net/encodings/o200k_base.tiktoken",
    "cl100k_base.tkz": "https://huggingface.co/tokiers/cl100k/resolve/main/tokenizer.tkz",
    "o200k_base.tkz": "https://huggingface.co/tokiers/o200k/resolve/main/tokenizer.tkz",
}

@dataclass(frozen=True)
class JsonAssetSpec:
    """Describe one Hugging Face tokenizer JSON conversion target.
    描述单个 Hugging Face tokenizer JSON 转换目标。
    """

    repo: str
    source_file: str = "tokenizer.json"
    revision: str = "main"
    sha256: str | None = None


@dataclass(frozen=True)
class DirectAssetSpec:
    """Describe one directly downloaded tokenizer asset.
    描述单个直接下载的 tokenizer 资产。
    """

    url: str
    sha256: str | None = None


# Exact tokenizer resources converted from the newest suitable public tokenizer JSON.
# 从最新且合适的公开 tokenizer JSON 转换得到的精确 tokenizer 资源。
HF_JSON_TOKENIZER_SPECS = {
    "qwen": JsonAssetSpec("Qwen/Qwen3.6-35B-A3B", revision="995ad96eacd98c81ed38be0c5b274b04031597b0"),
    "qwen_coder": JsonAssetSpec("Qwen/Qwen3-Coder-Next", revision="a7fbcb5c0e12d62a448eaa0e260346bf5dcc0feb"),
    "deepseek": JsonAssetSpec("deepseek-ai/DeepSeek-V4-Pro", revision="89d501aed998d33fa4f4702102ec1bb2331e10f6"),
    "deepseek_r1": JsonAssetSpec("deepseek-ai/DeepSeek-R1", revision="56d4cbbb4d29f4355bab4b9a39ccb717a14ad5ad"),
    "minimax": JsonAssetSpec("MiniMaxAI/MiniMax-M2.7", revision="d494266a4affc0d2995ba1fa35c8481cbd84294b"),
    "glm": JsonAssetSpec("zai-org/GLM-5.1", revision="26e1bd6e011feb778d25ae34b09b07074139d92d"),
    "hunyuan": JsonAssetSpec("tencent/Hy3-preview", revision="549c2b3a0fd5b9a6c6059a9935bf0d59ab69d75a"),
    "ernie": JsonAssetSpec("baidu/ERNIE-4.5-300B-A47B-PT", revision="47bbaf1c9e356313c3cb2ded9078a494fb152f0e"),
    "mistral": JsonAssetSpec("mistralai/Mistral-Medium-3.5-128B", revision="ed0c85631cf20929c29bc36d7cb37519ed0cdd28"),
    "phi": JsonAssetSpec("microsoft/Phi-4", revision="932b33c0ec9ca189badeb22480721a8de9d0e006"),
    "gemma": JsonAssetSpec("google/gemma-4-31B-it", revision="fb9ae262347c3945692f09a612f8bb189def854f"),
    "nemotron": JsonAssetSpec("nvidia/NVIDIA-Nemotron-3-Super-120B-A12B-FP8", revision="7d7e5797b8a3c7abbab54033b6004e93e8b6bc91"),
}

# Public latest fallback for gated Meta Llama 4 Maverick repositories.
# 用于受限 Meta Llama 4 Maverick 仓库的公开最新回退资源。
PUBLIC_TKZ_FALLBACK_URLS = {
    "llama/tokenizer.tkz": DirectAssetSpec(
        "https://huggingface.co/tokiers/Llama-4-Scout-17B-16E/resolve/3b0676d8dacdb40bac42709a2eb8014f8371025e/tokenizer.tkz"
    ),
}


def default_output_base() -> Path:
    """Return this crate's default tokenizer resource directory.
    返回本 crate 默认 tokenizer 资源目录。
    """
    return Path(__file__).resolve().parents[1] / "resources" / "tokenizers"

def crate_root() -> Path:
    """Return this crate's root directory.
    返回本 crate 的根目录。
    """
    return Path(__file__).resolve().parents[1]


def workspace_root() -> Path:
    """Return the workspace root directory.
    返回 workspace 根目录。
    """
    return crate_root().parents[1]


def download_file(
    url: str,
    destination: Path,
    *,
    force: bool = False,
    sha256: str | None = None,
) -> None:
    """Download one tokenizer file if it is not already present.
    在文件尚不存在时下载单个 tokenizer 文件。
    """
    if destination.exists() and destination.stat().st_size > 0 and not force:
        verify_sha256(destination, sha256)
        print(f"skip existing: {destination}")
        return

    destination.parent.mkdir(parents=True, exist_ok=True)
    print(f"download {url} -> {destination}")
    request = urllib.request.Request(url)
    if token := hf_token():
        request.add_header("Authorization", f"Bearer {token}")
    temporary_destination = destination.with_suffix(destination.suffix + ".tmp")
    with urllib.request.urlopen(request) as response:
        temporary_destination.write_bytes(response.read())
    verify_sha256(temporary_destination, sha256)
    temporary_destination.replace(destination)


def verify_sha256(path: Path, expected_sha256: str | None) -> None:
    """Verify one file against an optional SHA-256 checksum.
    使用可选 SHA-256 校验单个文件。
    """
    if expected_sha256 is None:
        return

    actual_sha256 = hashlib.sha256(path.read_bytes()).hexdigest()
    if actual_sha256.lower() != expected_sha256.lower():
        raise ValueError(
            f"sha256 mismatch for {path}: expected {expected_sha256}, got {actual_sha256}"
        )


def hf_token() -> str | None:
    """Return the optional Hugging Face token from the environment.
    从环境变量返回可选 Hugging Face token。
    """
    import os

    return os.environ.get("HF_TOKEN") or os.environ.get("HUGGINGFACE_HUB_TOKEN")


def hf_resolve_url(repo: str, revision: str, file_name: str) -> str:
    """Build one Hugging Face resolve URL.
    构建单个 Hugging Face resolve URL。
    """
    return f"https://huggingface.co/{repo}/resolve/{revision}/{file_name}"

def convert_json_to_tkz(source_json: Path, destination_tkz: Path, *, force: bool = False) -> None:
    """Convert one downloaded tokenizer JSON file into `.tkz`.
    将单个已下载 tokenizer JSON 文件转换为 `.tkz`。
    """
    if destination_tkz.exists() and destination_tkz.stat().st_size > 0 and not force:
        print(f"skip existing: {destination_tkz}")
        return

    destination_tkz.parent.mkdir(parents=True, exist_ok=True)
    print(f"convert {source_json} -> {destination_tkz}")
    subprocess.run(
        [
            "cargo",
            "run",
            "-p",
            "vulcan-token-counter",
            "--bin",
            "convert_tokenizer_json_to_tkz",
            "--",
            str(source_json),
            str(destination_tkz),
        ],
        cwd=workspace_root(),
        check=True,
    )

def remove_intermediate_json(source_json: Path) -> None:
    """Remove one intermediate tokenizer JSON file after successful conversion.
    在成功转换后移除单个中间 tokenizer JSON 文件。
    """
    if source_json.exists():
        print(f"remove intermediate: {source_json}")
        source_json.unlink()


def parse_args() -> argparse.Namespace:
    """Parse command-line arguments for the resource downloader.
    解析资源下载器命令行参数。
    """
    parser = argparse.ArgumentParser(
        description="Download minimal OpenAI tokenizer resources for vulcan-token-counter."
    )
    parser.add_argument(
        "--output-base",
        type=Path,
        default=default_output_base(),
        help="Tokenizer resource root; defaults to this crate's resources/tokenizers directory.",
    )
    parser.add_argument(
        "--keep-source-json",
        action="store_true",
        help="Keep downloaded tokenizer.json files after converting them to .tkz.",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="Re-download source assets and rebuild .tkz files even when outputs already exist.",
    )
    return parser.parse_args()


def main() -> None:
    """Download all required tokenizer resources.
    下载全部必需 tokenizer 资源。
    """
    args = parse_args()
    openai_dir = args.output_base / "openai"
    exact_dir = args.output_base / "exact"
    source_dir = args.output_base / "source"

    for file_name, url in OPENAI_TOKENIZER_URLS.items():
        download_file(url, openai_dir / file_name, force=args.force)

    for relative_path, spec in PUBLIC_TKZ_FALLBACK_URLS.items():
        download_file(spec.url, exact_dir / relative_path, force=args.force, sha256=spec.sha256)

    for key, spec in HF_JSON_TOKENIZER_SPECS.items():
        source_json = source_dir / key / spec.source_file
        destination_tkz = exact_dir / key / "tokenizer.tkz"
        download_file(
            hf_resolve_url(spec.repo, spec.revision, spec.source_file),
            source_json,
            force=args.force,
            sha256=spec.sha256,
        )
        convert_json_to_tkz(source_json, destination_tkz, force=args.force)
        if not args.keep_source_json:
            remove_intermediate_json(source_json)


if __name__ == "__main__":
    main()
