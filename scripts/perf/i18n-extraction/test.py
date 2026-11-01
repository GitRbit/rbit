#!/usr/bin/env python3
"""Run offline protocol/statistical tests; never build, launch Rbit or benchmark."""
import sys
from pathlib import Path


def main():
    if sys.version_info < (3, 11):
        raise SystemExit("i18n extraction tests require Python 3.11 or newer")
    import unittest

    directory = Path(__file__).resolve().parent
    sys.path.insert(0, str(directory))
    suite = unittest.defaultTestLoader.discover(str(directory), pattern="test_*.py")
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
