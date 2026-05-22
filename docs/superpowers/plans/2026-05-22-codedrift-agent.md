# agenomic-codedrift Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a single-shot Python LangGraph agent (`agenomic-codedrift`) that scores Claude code-quality drift on a fixed 5-snippet benchmark and exercises Agenomic end-to-end (bundle build/sign/push + traces + attestation).

**Architecture:** 5-node LangGraph `StateGraph` (`load_benchmark → prompt_claude → score_quality → compare_baseline → emit_trace`) wrapped by `@trace_agent_run` so the whole run produces one signed `TraceEnvelope`. Quality metrics computed via subprocess calls to `ruff`, `radon`, and `bandit`. Continuous trigger delegated to a GitHub Actions cron.

**Tech Stack:** Python 3.12, LangGraph (StateGraph), agenomic SDK (`@trace_agent_run`, `instrument_langgraph`, `AgenomicClient`), Anthropic SDK (`anthropic` 0.45+), pytest, ruff/radon/bandit (subprocess), uv for dep management.

---

## File map

```
agenomic-codedrift/
├── README.md                          # Task 14
├── Makefile                           # Task 14
├── pyproject.toml                     # Task 0
├── agenomic.yaml                      # Task 9
├── .gitignore                         # Task 0
├── benchmarks/                        # Task 1
│   ├── fizzbuzz.py
│   ├── two_sum.py
│   ├── parse_csv.py
│   ├── memo_fib.py
│   └── http_client.py
├── src/agenomic_codedrift/
│   ├── __init__.py                    # Task 0
│   ├── __main__.py                    # Task 7
│   ├── claude.py                      # Task 4
│   ├── metrics.py                     # Task 2
│   ├── baseline.py                    # Task 3
│   ├── nodes.py                       # Task 5
│   └── graph.py                       # Task 6
├── tests/
│   ├── __init__.py                    # Task 0
│   ├── test_metrics.py                # Task 2
│   ├── test_baseline.py               # Task 3
│   └── test_graph_smoke.py            # Task 8
└── .github/workflows/
    ├── ci.yml                         # Task 10
    ├── release.yml                    # Task 11
    └── drift.yml                      # Task 12
```

Parent-repo work (after submodule is created and pushed once):

```
.gitmodules                            # Task 13: add agenomic-codedrift entry
agenomic-codedrift                     # Task 13: submodule pointer
```

---

## Task 0: Bootstrap the new submodule repo

**Files:**
- Create: `agenomic-codedrift/pyproject.toml`
- Create: `agenomic-codedrift/.gitignore`
- Create: `agenomic-codedrift/src/agenomic_codedrift/__init__.py`
- Create: `agenomic-codedrift/tests/__init__.py`

- [ ] **Step 1: Create the directory and initialize git**

```bash
cd /Users/gabinmberikongo/code/treansai
mkdir -p agenomic-codedrift/{src/agenomic_codedrift,tests,benchmarks,.github/workflows,keys}
cd agenomic-codedrift
git init -b main
```

- [ ] **Step 2: Write `pyproject.toml`**

```toml
[project]
name = "agenomic-codedrift"
version = "0.1.0"
description = "E2E test agent — measures Claude code-quality drift over a fixed benchmark"
authors = [{ name = "Traidano", email = "dev@traidano.com" }]
license = { text = "Apache-2.0" }
requires-python = ">=3.12"
dependencies = [
  "agenomic[langgraph,anthropic] >= 0.1.0",
  "langgraph >= 0.2.0",
  "anthropic >= 0.45.0",
  "ruff >= 0.6.0",
  "radon >= 6.0.1",
  "bandit >= 1.7.9",
  "pydantic >= 2.0",
]

[project.optional-dependencies]
dev = [
  "pytest >= 8.3.0",
  "pytest-asyncio >= 0.24",
  "mypy >= 1.11",
]

[project.scripts]
agenomic-codedrift = "agenomic_codedrift.__main__:main"

[build-system]
requires = ["setuptools>=68"]
build-backend = "setuptools.build_meta"

[tool.setuptools.packages.find]
where = ["src"]

[tool.ruff]
line-length = 100
target-version = "py312"

[tool.ruff.lint]
select = ["E", "F", "W", "I", "B", "UP", "PL", "RUF"]
ignore = ["PLR0913"]  # too many args is fine for our small surface

[tool.mypy]
strict = true
python_version = "3.12"
files = ["src/agenomic_codedrift"]

[tool.pytest.ini_options]
testpaths = ["tests"]
asyncio_mode = "auto"
```

- [ ] **Step 3: Write `.gitignore`**

```gitignore
__pycache__/
*.pyc
.venv/
.pytest_cache/
.mypy_cache/
.ruff_cache/
dist/
build/
*.egg-info/

# Local agent state
traces/
baseline.jsonl
keys/*.priv

# IDE
.vscode/
.idea/
```

- [ ] **Step 4: Write package init files**

`src/agenomic_codedrift/__init__.py`:
```python
"""agenomic-codedrift: E2E test agent for the Agenomic platform."""

__version__ = "0.1.0"
```

`tests/__init__.py`: empty file.

- [ ] **Step 5: Install in editable mode and verify import**

```bash
python -m venv .venv
source .venv/bin/activate
pip install -e ".[dev]"
python -c "import agenomic_codedrift; print(agenomic_codedrift.__version__)"
```

Expected: `0.1.0`

- [ ] **Step 6: Commit**

```bash
git add .
git commit -m "chore: bootstrap agenomic-codedrift package skeleton"
```

---

## Task 1: Benchmark snippets

**Files:**
- Create: `benchmarks/fizzbuzz.py`
- Create: `benchmarks/two_sum.py`
- Create: `benchmarks/parse_csv.py`
- Create: `benchmarks/memo_fib.py`
- Create: `benchmarks/http_client.py`

These snippets are intentionally **un-idiomatic** — that's the substrate the LLM gets to "refactor for clarity", and that's how we get a measurable quality delta in the output.

- [ ] **Step 1: Write `benchmarks/fizzbuzz.py`**

```python
def f(n):
    r = []
    for i in range(1, n+1):
        if i%3==0 and i%5==0: r.append("FizzBuzz")
        elif i%3==0: r.append("Fizz")
        elif i%5==0: r.append("Buzz")
        else: r.append(str(i))
    return r
```

- [ ] **Step 2: Write `benchmarks/two_sum.py`**

```python
def s(nums, t):
    for i in range(len(nums)):
        for j in range(i+1, len(nums)):
            if nums[i]+nums[j]==t:
                return [i,j]
    return None
```

- [ ] **Step 3: Write `benchmarks/parse_csv.py`**

```python
def p(path):
    rows = []
    f = open(path)
    for line in f.readlines():
        rows.append(line.strip().split(","))
    f.close()
    return rows
```

- [ ] **Step 4: Write `benchmarks/memo_fib.py`**

```python
cache = {}
def fib(n):
    if n in cache: return cache[n]
    if n < 2: return n
    r = fib(n-1) + fib(n-2)
    cache[n] = r
    return r
```

- [ ] **Step 5: Write `benchmarks/http_client.py`**

```python
import urllib.request
def get(u):
    r = urllib.request.urlopen(u)
    d = r.read()
    return d.decode()
```

- [ ] **Step 6: Commit**

```bash
git add benchmarks/
git commit -m "feat: add 5 un-idiomatic benchmark snippets"
```

---

## Task 2: Quality metrics (TDD)

**Files:**
- Create: `src/agenomic_codedrift/metrics.py`
- Create: `tests/test_metrics.py`

- [ ] **Step 1: Write the failing tests**

`tests/test_metrics.py`:
```python
"""Quality-metric wrappers — ruff (lint), radon (complexity), bandit (security)."""

from __future__ import annotations

import textwrap
from pathlib import Path

import pytest

from agenomic_codedrift.metrics import (
    bandit_warnings,
    radon_complexity,
    ruff_issues,
    score_snippet,
)

CLEAN = textwrap.dedent(
    """
    def add(a: int, b: int) -> int:
        return a + b
    """
).strip()

DIRTY = textwrap.dedent(
    """
    def f(n):
        r=[]
        for i in range(1,n+1):
            if i%3==0:r.append('Fizz')
        return r
    """
).strip()

INSECURE = textwrap.dedent(
    """
    import subprocess
    def run(cmd):
        return subprocess.call(cmd, shell=True)
    """
).strip()


def test_ruff_issues_clean_returns_zero(tmp_path: Path) -> None:
    src = tmp_path / "x.py"
    src.write_text(CLEAN)
    assert ruff_issues(src) == 0


def test_ruff_issues_dirty_returns_positive(tmp_path: Path) -> None:
    src = tmp_path / "x.py"
    src.write_text(DIRTY)
    assert ruff_issues(src) > 0


def test_radon_complexity_simple_is_low(tmp_path: Path) -> None:
    src = tmp_path / "x.py"
    src.write_text(CLEAN)
    assert radon_complexity(src) == 1


def test_radon_complexity_branchy_is_higher(tmp_path: Path) -> None:
    src = tmp_path / "x.py"
    src.write_text(DIRTY)
    assert radon_complexity(src) >= 3


def test_bandit_warnings_clean_returns_zero(tmp_path: Path) -> None:
    src = tmp_path / "x.py"
    src.write_text(CLEAN)
    assert bandit_warnings(src) == 0


def test_bandit_warnings_insecure_returns_positive(tmp_path: Path) -> None:
    src = tmp_path / "x.py"
    src.write_text(INSECURE)
    assert bandit_warnings(src) > 0


def test_score_snippet_returns_all_three(tmp_path: Path) -> None:
    src = tmp_path / "x.py"
    src.write_text(CLEAN)
    score = score_snippet(src)
    assert score == {"lint_issues": 0, "complexity": 1, "security_warnings": 0}


def test_score_snippet_missing_tool_raises(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    src = tmp_path / "x.py"
    src.write_text(CLEAN)

    def boom(*a: object, **k: object) -> None:
        raise FileNotFoundError("ruff not on PATH")

    monkeypatch.setattr("agenomic_codedrift.metrics._run", boom)
    with pytest.raises(RuntimeError, match="ruff"):
        score_snippet(src)
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
pytest tests/test_metrics.py -v
```

Expected: 8 failures with `ModuleNotFoundError: No module named 'agenomic_codedrift.metrics'`.

- [ ] **Step 3: Implement `metrics.py`**

`src/agenomic_codedrift/metrics.py`:
```python
"""Quality-metric wrappers.

Each metric shells out to a CLI tool (ruff, radon, bandit) on a single
Python file and returns one integer summary. Subprocess errors that
look like "tool not on PATH" surface as RuntimeError with the tool
name in the message so the caller fails fast at startup time.
"""

from __future__ import annotations

import json
import subprocess
from pathlib import Path
from typing import TypedDict


class SnippetScore(TypedDict):
    lint_issues: int
    complexity: int
    security_warnings: int


def _run(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    try:
        return subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            check=False,
        )
    except FileNotFoundError as exc:
        raise RuntimeError(f"required tool not on PATH: {cmd[0]} ({exc})") from exc


def ruff_issues(path: Path) -> int:
    """Number of lint findings ruff emits for `path`. Zero == clean."""
    proc = _run(["ruff", "check", "--output-format=json", "--quiet", str(path)])
    if not proc.stdout.strip():
        return 0
    findings = json.loads(proc.stdout)
    return len(findings)


def radon_complexity(path: Path) -> int:
    """Sum of cyclomatic complexity across all functions in `path`."""
    proc = _run(["radon", "cc", "-s", "--json", str(path)])
    if not proc.stdout.strip():
        return 0
    data = json.loads(proc.stdout)
    blocks = data.get(str(path), [])
    if not isinstance(blocks, list):
        return 0
    return sum(int(b.get("complexity", 0)) for b in blocks)


def bandit_warnings(path: Path) -> int:
    """Number of security warnings bandit emits for `path`."""
    proc = _run(["bandit", "-f", "json", "-q", str(path)])
    if not proc.stdout.strip():
        return 0
    data = json.loads(proc.stdout)
    results = data.get("results", [])
    return len(results) if isinstance(results, list) else 0


def score_snippet(path: Path) -> SnippetScore:
    """Run all three metrics on `path` and return the combined dict."""
    return SnippetScore(
        lint_issues=ruff_issues(path),
        complexity=radon_complexity(path),
        security_warnings=bandit_warnings(path),
    )
```

- [ ] **Step 4: Run tests to verify pass**

```bash
pytest tests/test_metrics.py -v
```

Expected: 8 passed.

- [ ] **Step 5: Commit**

```bash
git add src/agenomic_codedrift/metrics.py tests/test_metrics.py
git commit -m "feat(metrics): ruff/radon/bandit wrappers with score_snippet"
```

---

## Task 3: Baseline persistence + drift math (TDD)

**Files:**
- Create: `src/agenomic_codedrift/baseline.py`
- Create: `tests/test_baseline.py`

- [ ] **Step 1: Write the failing tests**

`tests/test_baseline.py`:
```python
"""Baseline persistence + drift math."""

from __future__ import annotations

import json
from datetime import UTC, datetime, timedelta
from pathlib import Path

import pytest

from agenomic_codedrift.baseline import (
    BaselineEntry,
    DriftReport,
    append_entry,
    compute_drift,
    load_entries,
)


def _entry(
    when: datetime,
    snippet: str = "fizzbuzz",
    lint: int = 1,
    complexity: int = 2,
    security: int = 0,
) -> BaselineEntry:
    return BaselineEntry(
        timestamp=when.isoformat(),
        snippet=snippet,
        lint_issues=lint,
        complexity=complexity,
        security_warnings=security,
    )


def test_append_and_load_round_trip(tmp_path: Path) -> None:
    path = tmp_path / "baseline.jsonl"
    now = datetime.now(tz=UTC)
    e = _entry(now)
    append_entry(path, e)
    loaded = load_entries(path)
    assert loaded == [e]


def test_load_missing_file_returns_empty(tmp_path: Path) -> None:
    assert load_entries(tmp_path / "nope.jsonl") == []


def test_load_filters_by_max_age_days(tmp_path: Path) -> None:
    path = tmp_path / "baseline.jsonl"
    now = datetime.now(tz=UTC)
    fresh = _entry(now)
    stale = _entry(now - timedelta(days=45))
    append_entry(path, stale)
    append_entry(path, fresh)
    assert load_entries(path, max_age_days=30) == [fresh]


def test_drift_no_baseline_is_null(tmp_path: Path) -> None:
    current = _entry(datetime.now(tz=UTC), lint=5)
    report = compute_drift(history=[], current=current)
    assert report == DriftReport(
        snippet="fizzbuzz",
        lint_delta=None,
        complexity_delta=None,
        security_delta=None,
        flagged=False,
    )


def test_drift_within_one_stddev_not_flagged() -> None:
    now = datetime.now(tz=UTC)
    history = [_entry(now - timedelta(days=i), lint=4) for i in range(1, 11)]
    current = _entry(now, lint=4)
    report = compute_drift(history=history, current=current)
    assert report.lint_delta == 0.0
    assert report.flagged is False


def test_drift_beyond_one_stddev_flagged() -> None:
    now = datetime.now(tz=UTC)
    # baseline lint is consistently 1 (stddev 0) -> any nonzero delta is "infinite" stddev away
    history = [_entry(now - timedelta(days=i), lint=1) for i in range(1, 11)]
    current = _entry(now, lint=10)
    report = compute_drift(history=history, current=current)
    assert report.lint_delta == 9.0
    assert report.flagged is True


def test_drift_per_snippet_isolation() -> None:
    now = datetime.now(tz=UTC)
    history = [
        _entry(now - timedelta(days=1), snippet="other", lint=1),
        _entry(now - timedelta(days=2), snippet="other", lint=1),
    ]
    current = _entry(now, snippet="fizzbuzz", lint=10)
    report = compute_drift(history=history, current=current)
    # No history for *this* snippet -> null deltas
    assert report.lint_delta is None
    assert report.flagged is False


def test_compute_drift_invalid_history_raises() -> None:
    with pytest.raises(TypeError):
        compute_drift(history="not a list", current=_entry(datetime.now(tz=UTC)))  # type: ignore[arg-type]
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
pytest tests/test_baseline.py -v
```

Expected: 8 failures with `ModuleNotFoundError`.

- [ ] **Step 3: Implement `baseline.py`**

`src/agenomic_codedrift/baseline.py`:
```python
"""Baseline persistence + drift math.

The baseline is a JSONL file where each line is one (snippet, score)
observation. `compute_drift` flags a snippet when any metric deviates
by more than one standard deviation from the rolling mean of the
last 30 days for that same snippet.

First-run behaviour: with no history for the snippet, all deltas are
`None` and `flagged` is False.
"""

from __future__ import annotations

import json
import statistics
from dataclasses import asdict, dataclass
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import Optional


@dataclass(frozen=True)
class BaselineEntry:
    timestamp: str  # ISO 8601 UTC
    snippet: str
    lint_issues: int
    complexity: int
    security_warnings: int


@dataclass(frozen=True)
class DriftReport:
    snippet: str
    lint_delta: Optional[float]
    complexity_delta: Optional[float]
    security_delta: Optional[float]
    flagged: bool


def append_entry(path: Path, entry: BaselineEntry) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as fp:
        fp.write(json.dumps(asdict(entry)) + "\n")


def load_entries(path: Path, *, max_age_days: int | None = None) -> list[BaselineEntry]:
    if not path.exists():
        return []
    cutoff = (
        datetime.now(tz=UTC) - timedelta(days=max_age_days) if max_age_days else None
    )
    out: list[BaselineEntry] = []
    with path.open(encoding="utf-8") as fp:
        for line in fp:
            line = line.strip()
            if not line:
                continue
            raw = json.loads(line)
            entry = BaselineEntry(**raw)
            if cutoff is not None:
                ts = datetime.fromisoformat(entry.timestamp)
                if ts < cutoff:
                    continue
            out.append(entry)
    return out


def _delta(history_values: list[int], current_value: int) -> tuple[Optional[float], bool]:
    """Return (delta, flagged). Flagged when |delta| > 1 stddev (or any
    non-zero delta when stddev is 0 and history is non-empty)."""
    if not history_values:
        return (None, False)
    mean = statistics.fmean(history_values)
    delta = float(current_value) - mean
    if len(history_values) < 2:
        # Single observation — flag any non-zero delta as a conservative default.
        return (delta, delta != 0.0)
    stddev = statistics.stdev(history_values)
    if stddev == 0.0:
        return (delta, delta != 0.0)
    flagged = abs(delta) > stddev
    return (delta, flagged)


def compute_drift(
    *, history: list[BaselineEntry], current: BaselineEntry
) -> DriftReport:
    if not isinstance(history, list):
        raise TypeError("history must be a list of BaselineEntry")
    same_snippet = [h for h in history if h.snippet == current.snippet]
    lint_d, lint_f = _delta([h.lint_issues for h in same_snippet], current.lint_issues)
    cx_d, cx_f = _delta([h.complexity for h in same_snippet], current.complexity)
    sec_d, sec_f = _delta(
        [h.security_warnings for h in same_snippet], current.security_warnings
    )
    return DriftReport(
        snippet=current.snippet,
        lint_delta=lint_d,
        complexity_delta=cx_d,
        security_delta=sec_d,
        flagged=lint_f or cx_f or sec_f,
    )
```

- [ ] **Step 4: Run tests to verify pass**

```bash
pytest tests/test_baseline.py -v
```

Expected: 8 passed.

- [ ] **Step 5: Commit**

```bash
git add src/agenomic_codedrift/baseline.py tests/test_baseline.py
git commit -m "feat(baseline): JSONL persistence + per-snippet drift math"
```

---

## Task 4: Claude client wrapper

**Files:**
- Create: `src/agenomic_codedrift/claude.py`

No dedicated unit test for this module — it's a thin Anthropic SDK wrapper. It's exercised by the integration test in Task 8 via a mocked Anthropic client.

- [ ] **Step 1: Implement `claude.py`**

`src/agenomic_codedrift/claude.py`:
```python
"""Thin wrapper around the Anthropic SDK with retry/backoff."""

from __future__ import annotations

import logging
import os
import time
from dataclasses import dataclass

import anthropic
from anthropic import APIStatusError, APITimeoutError

logger = logging.getLogger(__name__)

DEFAULT_MODEL = "claude-sonnet-4-6"
DEFAULT_MAX_TOKENS = 1024
PROMPT_TEMPLATE = (
    "Refactor the following Python function for clarity and add complete "
    "type hints. Return only valid Python code, no commentary, no markdown "
    "fences.\n\n```python\n{source}\n```"
)


@dataclass(frozen=True)
class ClaudeResponse:
    model: str
    text: str
    input_tokens: int
    output_tokens: int


def make_client(api_key: str | None = None) -> anthropic.Anthropic:
    return anthropic.Anthropic(api_key=api_key or os.environ["ANTHROPIC_API_KEY"])


def refactor_snippet(
    client: anthropic.Anthropic,
    source: str,
    *,
    model: str = DEFAULT_MODEL,
    max_retries: int = 3,
) -> ClaudeResponse:
    """Ask Claude to refactor `source`. Exponential backoff on 429/timeout."""
    delays = [1, 4, 16]
    last_exc: Exception | None = None
    for attempt in range(max_retries):
        try:
            msg = client.messages.create(
                model=model,
                max_tokens=DEFAULT_MAX_TOKENS,
                messages=[{"role": "user", "content": PROMPT_TEMPLATE.format(source=source)}],
            )
            text_parts = [b.text for b in msg.content if getattr(b, "type", "") == "text"]
            return ClaudeResponse(
                model=msg.model,
                text="".join(text_parts).strip(),
                input_tokens=msg.usage.input_tokens,
                output_tokens=msg.usage.output_tokens,
            )
        except (APIStatusError, APITimeoutError) as exc:
            last_exc = exc
            if attempt == max_retries - 1:
                break
            logger.warning(
                "claude call attempt %d failed (%s); retrying in %ds",
                attempt + 1,
                type(exc).__name__,
                delays[attempt],
            )
            time.sleep(delays[attempt])
    assert last_exc is not None
    raise last_exc
```

- [ ] **Step 2: Commit**

```bash
git add src/agenomic_codedrift/claude.py
git commit -m "feat(claude): thin Anthropic wrapper with exponential backoff"
```

---

## Task 5: Node implementations

**Files:**
- Create: `src/agenomic_codedrift/nodes.py`

The 5 nodes operate on a shared `GraphState` `TypedDict`. Each node is a pure function `GraphState -> GraphState`. Side effects (file IO, network) live in the leaf modules (`metrics`, `baseline`, `claude`) — never inside the node closure.

- [ ] **Step 1: Implement `nodes.py`**

`src/agenomic_codedrift/nodes.py`:
```python
"""LangGraph node implementations."""

from __future__ import annotations

import logging
import tempfile
from datetime import UTC, datetime
from pathlib import Path
from typing import TypedDict

import anthropic

from agenomic_codedrift.baseline import (
    BaselineEntry,
    DriftReport,
    append_entry,
    compute_drift,
    load_entries,
)
from agenomic_codedrift.claude import refactor_snippet
from agenomic_codedrift.metrics import score_snippet

logger = logging.getLogger(__name__)


class Snippet(TypedDict):
    name: str
    source: str


class Scored(TypedDict):
    name: str
    source: str
    response: str
    lint_issues: int
    complexity: int
    security_warnings: int
    error: bool


class GraphState(TypedDict, total=False):
    # inputs
    benchmark_dir: Path
    baseline_path: Path
    claude_client: anthropic.Anthropic
    # intermediate
    snippets: list[Snippet]
    scored: list[Scored]
    drift_reports: list[DriftReport]
    # output
    run_started_at: str


def load_benchmark(state: GraphState) -> GraphState:
    """Read every *.py file under benchmark_dir into a snippet list."""
    bench = state["benchmark_dir"]
    snippets: list[Snippet] = []
    for p in sorted(bench.glob("*.py")):
        snippets.append({"name": p.stem, "source": p.read_text(encoding="utf-8")})
    if not snippets:
        raise RuntimeError(f"no benchmark snippets found under {bench}")
    return {
        **state,
        "snippets": snippets,
        "run_started_at": datetime.now(tz=UTC).isoformat(),
    }


def prompt_claude(state: GraphState) -> GraphState:
    """Ask Claude to refactor each snippet. Errors per snippet become
    `error: True` placeholders so the run continues."""
    client = state["claude_client"]
    scored: list[Scored] = []
    for snip in state["snippets"]:
        try:
            resp = refactor_snippet(client, snip["source"])
            scored.append(
                {
                    "name": snip["name"],
                    "source": snip["source"],
                    "response": resp.text,
                    "lint_issues": 0,
                    "complexity": 0,
                    "security_warnings": 0,
                    "error": False,
                }
            )
        except Exception as exc:
            logger.exception("snippet %s failed: %s", snip["name"], exc)
            scored.append(
                {
                    "name": snip["name"],
                    "source": snip["source"],
                    "response": "",
                    "lint_issues": 0,
                    "complexity": 0,
                    "security_warnings": 0,
                    "error": True,
                }
            )
    return {**state, "scored": scored}


def score_quality(state: GraphState) -> GraphState:
    """Run ruff/radon/bandit on each Claude response."""
    enriched: list[Scored] = []
    for s in state["scored"]:
        if s["error"] or not s["response"]:
            enriched.append(s)
            continue
        with tempfile.NamedTemporaryFile("w", suffix=".py", delete=False) as fp:
            fp.write(s["response"])
            tmp = Path(fp.name)
        try:
            score = score_snippet(tmp)
        finally:
            tmp.unlink(missing_ok=True)
        enriched.append({**s, **score})
    return {**state, "scored": enriched}


def compare_baseline(state: GraphState) -> GraphState:
    """Diff current scores against the rolling 30-day baseline."""
    history = load_entries(state["baseline_path"], max_age_days=30)
    reports: list[DriftReport] = []
    for s in state["scored"]:
        if s["error"]:
            reports.append(
                DriftReport(
                    snippet=s["name"],
                    lint_delta=None,
                    complexity_delta=None,
                    security_delta=None,
                    flagged=False,
                )
            )
            continue
        current = BaselineEntry(
            timestamp=state["run_started_at"],
            snippet=s["name"],
            lint_issues=s["lint_issues"],
            complexity=s["complexity"],
            security_warnings=s["security_warnings"],
        )
        reports.append(compute_drift(history=history, current=current))
    return {**state, "drift_reports": reports}


def emit_trace(state: GraphState) -> GraphState:
    """Append current scores to the baseline file. The TraceEnvelope
    itself is produced by @trace_agent_run, which wraps the graph
    invocation — this node only writes the side-effecting baseline."""
    for s in state["scored"]:
        if s["error"]:
            continue
        entry = BaselineEntry(
            timestamp=state["run_started_at"],
            snippet=s["name"],
            lint_issues=s["lint_issues"],
            complexity=s["complexity"],
            security_warnings=s["security_warnings"],
        )
        append_entry(state["baseline_path"], entry)
    return state
```

- [ ] **Step 2: Commit**

```bash
git add src/agenomic_codedrift/nodes.py
git commit -m "feat(nodes): 5 LangGraph nodes (load/prompt/score/compare/emit)"
```

---

## Task 6: Graph wiring

**Files:**
- Create: `src/agenomic_codedrift/graph.py`

- [ ] **Step 1: Implement `graph.py`**

`src/agenomic_codedrift/graph.py`:
```python
"""LangGraph StateGraph wiring for the 5 codedrift nodes."""

from __future__ import annotations

from langgraph.graph import END, START, StateGraph

from agenomic_codedrift.nodes import (
    GraphState,
    compare_baseline,
    emit_trace,
    load_benchmark,
    prompt_claude,
    score_quality,
)


def build_graph() -> "object":
    """Return a compiled LangGraph with the 5 codedrift nodes wired in
    sequence. Edges: START → load → prompt → score → compare → emit → END."""
    g: StateGraph = StateGraph(GraphState)
    g.add_node("load_benchmark", load_benchmark)
    g.add_node("prompt_claude", prompt_claude)
    g.add_node("score_quality", score_quality)
    g.add_node("compare_baseline", compare_baseline)
    g.add_node("emit_trace", emit_trace)

    g.add_edge(START, "load_benchmark")
    g.add_edge("load_benchmark", "prompt_claude")
    g.add_edge("prompt_claude", "score_quality")
    g.add_edge("score_quality", "compare_baseline")
    g.add_edge("compare_baseline", "emit_trace")
    g.add_edge("emit_trace", END)

    return g.compile()
```

- [ ] **Step 2: Commit**

```bash
git add src/agenomic_codedrift/graph.py
git commit -m "feat(graph): wire 5 nodes into a sequential LangGraph"
```

---

## Task 7: CLI entry

**Files:**
- Create: `src/agenomic_codedrift/__main__.py`

- [ ] **Step 1: Implement `__main__.py`**

`src/agenomic_codedrift/__main__.py`:
```python
"""CLI entry point: `python -m agenomic_codedrift run`."""

from __future__ import annotations

import argparse
import asyncio
import logging
import os
import sys
from pathlib import Path

from agenomic.client.client import AgenomicClient
from agenomic.exporters.jsonl import JsonlExporter
from agenomic.trace.decorator import trace_agent_run

from agenomic_codedrift import __version__
from agenomic_codedrift.claude import make_client
from agenomic_codedrift.graph import build_graph
from agenomic_codedrift.nodes import GraphState

AGENT_ID = "agent://traidano/codedrift"


def _setup_logging() -> None:
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    )


async def _maybe_upload(jsonl_path: Path) -> None:
    endpoint = os.environ.get("AGENOMIC_ENDPOINT")
    api_key = os.environ.get("AGENOMIC_API_KEY")
    if not endpoint or not api_key:
        logging.info("AGENOMIC_ENDPOINT/AGENOMIC_API_KEY unset — skipping upload")
        return
    from agenomic.types.envelope import TraceEnvelope
    import json

    envelopes: list[TraceEnvelope] = []
    with jsonl_path.open(encoding="utf-8") as fp:
        for line in fp:
            line = line.strip()
            if line:
                envelopes.append(TraceEnvelope.model_validate(json.loads(line)))
    client = AgenomicClient(endpoint, api_key)
    try:
        result = await client.upload_traces(envelopes)
        logging.info("uploaded %d envelopes: %s", len(envelopes), result)
    finally:
        await client.aclose()


def run_once(*, benchmark_dir: Path, baseline_path: Path, traces_path: Path) -> int:
    """Execute the codedrift graph once. Returns process exit code."""
    claude_client = make_client()
    release_id = os.environ.get("AGENOMIC_RELEASE_ID")

    with JsonlExporter(str(traces_path)) as exporter:

        @trace_agent_run(agent_id=AGENT_ID, release=release_id, exporter=exporter)
        def _run() -> GraphState:
            graph = build_graph()
            state: GraphState = {
                "benchmark_dir": benchmark_dir,
                "baseline_path": baseline_path,
                "claude_client": claude_client,
            }
            return graph.invoke(state)  # type: ignore[no-any-return]

        final = _run()

    print("\n=== codedrift run summary ===")
    for report in final.get("drift_reports", []):
        flag = " FLAGGED" if report.flagged else ""
        print(f"  {report.snippet:20s} lint={report.lint_delta} cx={report.complexity_delta} sec={report.security_delta}{flag}")

    try:
        asyncio.run(_maybe_upload(traces_path))
    except Exception:
        logging.exception("trace upload failed — local JSONL is at %s", traces_path)
        return 1
    return 0


def main(argv: list[str] | None = None) -> int:
    _setup_logging()
    parser = argparse.ArgumentParser(prog="agenomic-codedrift")
    parser.add_argument("--version", action="version", version=__version__)
    sub = parser.add_subparsers(dest="cmd", required=True)
    run = sub.add_parser("run", help="run the codedrift agent once")
    run.add_argument("--benchmark-dir", type=Path, default=Path("benchmarks"))
    run.add_argument("--baseline-path", type=Path, default=Path("baseline.jsonl"))
    run.add_argument("--traces-path", type=Path, default=Path("traces") / "run.jsonl")
    args = parser.parse_args(argv)

    if args.cmd == "run":
        return run_once(
            benchmark_dir=args.benchmark_dir,
            baseline_path=args.baseline_path,
            traces_path=args.traces_path,
        )
    return 2


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 2: Smoke-test the CLI (without LLM)**

```bash
python -m agenomic_codedrift --version
```

Expected: `0.1.0`

- [ ] **Step 3: Commit**

```bash
git add src/agenomic_codedrift/__main__.py
git commit -m "feat(cli): python -m agenomic_codedrift run + trace upload"
```

---

## Task 8: Integration test with mocked Claude

**Files:**
- Create: `tests/test_graph_smoke.py`

- [ ] **Step 1: Write the failing test**

`tests/test_graph_smoke.py`:
```python
"""End-to-end graph smoke test with Claude mocked."""

from __future__ import annotations

import json
import textwrap
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import MagicMock

import pytest

from agenomic_codedrift.graph import build_graph
from agenomic_codedrift.nodes import GraphState


@pytest.fixture
def benchmark_dir(tmp_path: Path) -> Path:
    d = tmp_path / "bench"
    d.mkdir()
    (d / "fizzbuzz.py").write_text(
        textwrap.dedent(
            """
            def f(n):
                r=[]
                for i in range(1,n+1):
                    if i%3==0:r.append('Fizz')
                return r
            """
        ).strip()
    )
    return d


@pytest.fixture
def mock_claude() -> MagicMock:
    """Stand-in for anthropic.Anthropic with a canned refactor response."""
    client = MagicMock()
    response_text = textwrap.dedent(
        """
        def fizz(n: int) -> list[str]:
            result: list[str] = []
            for i in range(1, n + 1):
                if i % 3 == 0:
                    result.append("Fizz")
            return result
        """
    ).strip()
    client.messages.create.return_value = SimpleNamespace(
        model="claude-sonnet-4-6-mock",
        content=[SimpleNamespace(type="text", text=response_text)],
        usage=SimpleNamespace(input_tokens=42, output_tokens=17),
    )
    return client


def test_graph_runs_end_to_end(benchmark_dir: Path, tmp_path: Path, mock_claude: MagicMock) -> None:
    baseline = tmp_path / "baseline.jsonl"
    graph = build_graph()
    state: GraphState = {
        "benchmark_dir": benchmark_dir,
        "baseline_path": baseline,
        "claude_client": mock_claude,
    }
    final = graph.invoke(state)

    assert final["snippets"][0]["name"] == "fizzbuzz"
    assert final["scored"][0]["error"] is False
    assert final["scored"][0]["response"].startswith("def fizz")
    # First run -> no history -> drift deltas are None
    assert final["drift_reports"][0].lint_delta is None
    assert final["drift_reports"][0].flagged is False
    # Baseline file was appended to
    assert baseline.exists()
    assert json.loads(baseline.read_text().splitlines()[0])["snippet"] == "fizzbuzz"


def test_graph_handles_claude_failure(benchmark_dir: Path, tmp_path: Path) -> None:
    client = MagicMock()
    client.messages.create.side_effect = RuntimeError("boom")
    graph = build_graph()
    final = graph.invoke(
        {
            "benchmark_dir": benchmark_dir,
            "baseline_path": tmp_path / "baseline.jsonl",
            "claude_client": client,
        }
    )
    # Snippet was marked error, the run still completed
    assert final["scored"][0]["error"] is True
    assert final["drift_reports"][0].flagged is False
```

- [ ] **Step 2: Run tests**

```bash
pytest tests/test_graph_smoke.py -v
```

Expected: 2 passed.

- [ ] **Step 3: Commit**

```bash
git add tests/test_graph_smoke.py
git commit -m "test(graph): end-to-end smoke with mocked Claude"
```

---

## Task 9: Agenomic bundle manifest + signing key

**Files:**
- Create: `agenomic.yaml`
- Create: `keys/.gitkeep`

- [ ] **Step 1: Write `agenomic.yaml`**

```yaml
name: codedrift-agent
version: 0.1.0
description: E2E test agent — measures Claude code-quality drift over a fixed benchmark
authors:
  - traidano
runtime: python
entrypoint: agenomic_codedrift.__main__:main
inputs:
  schema:
    snippets: list
    model: string
outputs:
  schema:
    drift_flags: list
    per_metric: object
signing:
  algorithm: ed25519
  key: keys/codedrift.priv
```

- [ ] **Step 2: Create the `keys/` placeholder**

```bash
mkdir -p keys
echo "# ed25519 signing keys live here. Real keys are gitignored." > keys/.gitkeep
```

- [ ] **Step 3: Generate a local signing key (operator-only, not committed)**

```bash
python -c "
from nacl.signing import SigningKey
import base64
sk = SigningKey.generate()
with open('keys/codedrift.priv', 'wb') as f:
    f.write(sk.encode())
with open('keys/codedrift.pub', 'wb') as f:
    f.write(sk.verify_key.encode())
print('signing key written to keys/codedrift.priv (private — never commit)')
"
```

If `nacl` is not installed: `pip install pynacl` first.

- [ ] **Step 4: Commit manifest + placeholder (NOT the private key)**

```bash
git add agenomic.yaml keys/.gitkeep
git status  # confirm keys/codedrift.priv is NOT staged (gitignored)
git commit -m "feat(bundle): agenomic.yaml manifest + signing key placeholder"
```

---

## Task 10: CI workflow

**Files:**
- Create: `.github/workflows/ci.yml`

- [ ] **Step 1: Write CI workflow**

```yaml
name: ci

on:
  push:
    branches: [main, develop]
  pull_request:

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v5
        with: { python-version: '3.12' }
      - name: Install
        run: pip install -e ".[dev]"
      - name: Lint
        run: ruff check src tests
      - name: Typecheck
        run: mypy src/agenomic_codedrift
      - name: Test
        run: pytest -v
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: pytest + ruff + mypy on push/PR"
```

---

## Task 11: Release workflow (bundle push)

**Files:**
- Create: `.github/workflows/release.yml`

This workflow uses the SDK directly to package + upload the bundle (the SDK exposes `AgenomicClient.upload_bundle(agent_id, archive_path)` and `create_release`). The signing key is provisioned via a GitHub Actions secret.

- [ ] **Step 1: Write the release helper script**

`scripts/build_and_push_bundle.py`:
```python
"""Package the working dir into an Agenomic bundle, sign it, push to cloud.

Reads:
  AGENOMIC_ENDPOINT, AGENOMIC_API_KEY, AGENOMIC_ORG_ID
  AGENOMIC_SIGNING_KEY_B64 (base64-encoded ed25519 private key)
  GITHUB_REF_NAME           (the tag, e.g. v0.1.0)

Writes:
  bundle.tar.gz
  prints the release_id to stdout, also sets `release_id` GHA output if
  GITHUB_OUTPUT is set.
"""

from __future__ import annotations

import asyncio
import base64
import os
import sys
import tarfile
from pathlib import Path

from agenomic.client.client import AgenomicClient

AGENT_ID = "agent://traidano/codedrift"
BUNDLE_INCLUDE = ["src", "benchmarks", "agenomic.yaml", "pyproject.toml", "README.md"]


def make_archive(out: Path) -> Path:
    out.parent.mkdir(parents=True, exist_ok=True)
    with tarfile.open(out, "w:gz") as tar:
        for item in BUNDLE_INCLUDE:
            p = Path(item)
            if p.exists():
                tar.add(p, arcname=item)
    return out


async def push(archive: Path) -> dict[str, object]:
    endpoint = os.environ["AGENOMIC_ENDPOINT"]
    api_key = os.environ["AGENOMIC_API_KEY"]
    client = AgenomicClient(endpoint, api_key)
    try:
        upload_result = await client.upload_bundle(AGENT_ID, archive)
        bundle_id = upload_result["bundle"]["id"]
        version = os.environ.get("GITHUB_REF_NAME", "v0.0.0-local").lstrip("v")
        release = await client.create_release(
            {
                "agent_id": AGENT_ID,
                "bundle_id": bundle_id,
                "version": version,
                "notes": f"automated release from {os.environ.get('GITHUB_SHA', 'local')}",
            }
        )
        return release
    finally:
        await client.aclose()


def main() -> int:
    archive = make_archive(Path("dist/bundle.tar.gz"))
    print(f"packaged bundle: {archive} ({archive.stat().st_size} bytes)")

    # Decode + persist the signing key so the SDK's bundle signer picks it up.
    key_b64 = os.environ.get("AGENOMIC_SIGNING_KEY_B64")
    if key_b64:
        Path("keys").mkdir(exist_ok=True)
        Path("keys/codedrift.priv").write_bytes(base64.b64decode(key_b64))

    release = asyncio.run(push(archive))
    release_id = release.get("release", {}).get("id")
    print(f"release_id={release_id}")

    out_path = os.environ.get("GITHUB_OUTPUT")
    if out_path and release_id:
        with open(out_path, "a", encoding="utf-8") as fp:
            fp.write(f"release_id={release_id}\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 2: Write the workflow**

`.github/workflows/release.yml`:
```yaml
name: release

on:
  push:
    tags: ['v*']
  workflow_dispatch:

jobs:
  bundle-push:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v5
        with: { python-version: '3.12' }
      - name: Install
        run: pip install -e .
      - name: Build + push bundle
        id: push
        env:
          AGENOMIC_ENDPOINT:        ${{ secrets.AGENOMIC_ENDPOINT }}
          AGENOMIC_API_KEY:         ${{ secrets.AGENOMIC_API_KEY }}
          AGENOMIC_ORG_ID:          ${{ secrets.AGENOMIC_ORG_ID }}
          AGENOMIC_SIGNING_KEY_B64: ${{ secrets.AGENOMIC_SIGNING_KEY_B64 }}
        run: python scripts/build_and_push_bundle.py
      - name: Save release_id
        run: echo "Released ${{ steps.push.outputs.release_id }}"
```

- [ ] **Step 3: Commit**

```bash
mkdir -p scripts
git add scripts/build_and_push_bundle.py .github/workflows/release.yml
git commit -m "ci(release): on tag, package + push signed bundle to Agenomic"
```

---

## Task 12: Drift cron workflow

**Files:**
- Create: `.github/workflows/drift.yml`

- [ ] **Step 1: Write the workflow**

```yaml
name: drift

on:
  schedule:
    - cron: '0 */6 * * *'
  workflow_dispatch:

jobs:
  run:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v5
        with: { python-version: '3.12' }
      - name: Install
        run: pip install -e .
      - name: Run codedrift
        env:
          ANTHROPIC_API_KEY:   ${{ secrets.ANTHROPIC_API_KEY }}
          AGENOMIC_ENDPOINT:   ${{ secrets.AGENOMIC_ENDPOINT }}
          AGENOMIC_API_KEY:    ${{ secrets.AGENOMIC_API_KEY }}
          AGENOMIC_RELEASE_ID: ${{ secrets.AGENOMIC_RELEASE_ID }}
        run: python -m agenomic_codedrift run
      - name: Upload trace JSONL on failure
        if: failure()
        uses: actions/upload-artifact@v4
        with:
          name: traces
          path: traces/
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/drift.yml
git commit -m "ci(drift): cron 6h runs the agent against prod cloud"
```

---

## Task 13: Wire the submodule into the parent agenomic repo

**Files:**
- Create on GitHub: `treansai/agenomic-codedrift` (new empty repo)
- Modify: `/Users/gabinmberikongo/code/treansai/agenomic/.gitmodules`

- [ ] **Step 1: Push the new repo to GitHub**

In the submodule dir:
```bash
cd /Users/gabinmberikongo/code/treansai/agenomic-codedrift
gh repo create treansai/agenomic-codedrift --private --source=. --remote=origin --push
```

- [ ] **Step 2: Create develop branch (matches the gitflow convention used in agenomic-cloud + agenomic-web)**

```bash
git branch develop
git push -u origin develop
```

- [ ] **Step 3: Register as submodule in the parent repo**

```bash
cd /Users/gabinmberikongo/code/treansai/agenomic
git submodule add -b main https://github.com/treansai/agenomic-codedrift.git agenomic-codedrift
```

- [ ] **Step 4: Commit the parent**

```bash
git add .gitmodules agenomic-codedrift
git commit -m "chore(submodules): add agenomic-codedrift (E2E test agent)"
git push origin main
```

---

## Task 14: README + Makefile + final polish

**Files:**
- Create: `agenomic-codedrift/README.md`
- Create: `agenomic-codedrift/Makefile`

- [ ] **Step 1: Write `Makefile`**

```makefile
.PHONY: install test lint typecheck e2e bundle clean

install:
	pip install -e ".[dev]"

test:
	pytest -v

lint:
	ruff check src tests

typecheck:
	mypy src/agenomic_codedrift

ci: lint typecheck test

e2e:
	@test -n "$$ANTHROPIC_API_KEY" || (echo "ANTHROPIC_API_KEY required" && exit 1)
	@test -n "$$AGENOMIC_API_KEY"   || (echo "AGENOMIC_API_KEY required"   && exit 1)
	@test -n "$$AGENOMIC_ENDPOINT"  || (echo "AGENOMIC_ENDPOINT required"  && exit 1)
	python -m agenomic_codedrift run --benchmark-dir benchmarks --baseline-path baseline.jsonl

bundle:
	python scripts/build_and_push_bundle.py

clean:
	rm -rf dist build .pytest_cache .mypy_cache .ruff_cache **/__pycache__
```

- [ ] **Step 2: Write `README.md`**

```markdown
# agenomic-codedrift

End-to-end test agent for the [Agenomic](https://agenomic.dev) platform.

A LangGraph agent that prompts Claude with a fixed benchmark of 5 Python
snippets, scores the refactored output with `ruff` / `radon` / `bandit`,
and reports per-metric drift versus a 30-day rolling baseline. Each run
emits a signed `TraceEnvelope` to Agenomic Cloud.

This project exists primarily to exercise the full Agenomic platform —
bundle versioning, signing, registry push, trace upload, attestation
linkage — on a continuous schedule (GitHub Actions cron, every 6 hours).

## Quickstart

```bash
git clone https://github.com/treansai/agenomic-codedrift
cd agenomic-codedrift
make install
make test
```

To run the agent against a real Anthropic + Agenomic backend:

```bash
export ANTHROPIC_API_KEY=sk-ant-...
export AGENOMIC_ENDPOINT=https://api.agenomic.example
export AGENOMIC_API_KEY=alk_...
make e2e
```

## Layout

| Path                                  | Purpose                                                   |
|---------------------------------------|-----------------------------------------------------------|
| `benchmarks/*.py`                     | 5 frozen un-idiomatic snippets                            |
| `src/agenomic_codedrift/graph.py`     | LangGraph wiring (5 sequential nodes)                     |
| `src/agenomic_codedrift/nodes.py`     | load → prompt → score → compare → emit                    |
| `src/agenomic_codedrift/metrics.py`   | ruff / radon / bandit subprocess wrappers                 |
| `src/agenomic_codedrift/baseline.py`  | JSONL persistence + per-snippet drift math                |
| `src/agenomic_codedrift/claude.py`    | Anthropic SDK wrapper with exponential backoff            |
| `agenomic.yaml`                       | Bundle manifest (version, signing, schema)                |
| `.github/workflows/`                  | CI, release (bundle push on tag), drift (cron 6h)         |

## Bundle release

Tag the repo to trigger the bundle push:

```bash
git tag v0.1.0
git push origin v0.1.0
```

`release.yml` packages `src/`, `benchmarks/`, `agenomic.yaml`, and
`pyproject.toml` into a tarball, signs it with the ed25519 key stored
in the `AGENOMIC_SIGNING_KEY_B64` GitHub secret, and pushes via
`AgenomicClient.upload_bundle` + `create_release`. The resulting
release id is exported as `AGENOMIC_RELEASE_ID` for the drift cron.

## License

Apache-2.0.
```

- [ ] **Step 3: Commit**

```bash
git add Makefile README.md
git commit -m "docs: README + Makefile (install/test/lint/e2e/bundle)"
git push origin main
```

- [ ] **Step 4: Bump the parent submodule pointer**

```bash
cd /Users/gabinmberikongo/code/treansai/agenomic
git add agenomic-codedrift
git commit -m "chore(submodules): bump agenomic-codedrift for v0.1.0 ready-to-tag state"
git push origin main
```

---

## Done. Verification checklist

- [ ] `pytest -v` in `agenomic-codedrift/` reports all tests passing (8 metrics + 8 baseline + 2 smoke = 18).
- [ ] `ruff check src tests` reports no findings.
- [ ] `mypy src/agenomic_codedrift` reports no errors.
- [ ] `python -m agenomic_codedrift --version` prints `0.1.0`.
- [ ] Locally with `ANTHROPIC_API_KEY` set and `AGENOMIC_*` unset, `make e2e` completes and writes `baseline.jsonl` + `traces/run.jsonl` (no upload).
- [ ] The parent agenomic repo has `agenomic-codedrift` as a registered submodule on `main`.
- [ ] GitHub repo `treansai/agenomic-codedrift` exists, `ci.yml` passes on first push.
