"""Root wrapper for the vulcan-token-counter tokenizer resource downloader.
vulcan-token-counter tokenizer 资源下载器的仓库根包装入口。
"""

from __future__ import annotations

import runpy
import sys
from pathlib import Path


def main() -> None:
    """Forward execution to the crate-owned downloader implementation.
    将执行转发到 crate 持有的下载器真实实现。
    """
    repository_root = Path(__file__).resolve().parents[1]
    implementation = (
        repository_root
        / "crates"
        / "vulcan-token-counter"
        / "scripts"
        / "fetch_tokenizers.py"
    )
    sys.argv[0] = str(implementation)
    runpy.run_path(str(implementation), run_name="__main__")


if __name__ == "__main__":
    main()
