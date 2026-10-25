"""Group measured speech tokens without estimating or changing their clocks.

The caller supplies text measurement in the same units as max_line_width.
Overlapping word spans remain overlapping; cue consumers choose display policy.
"""

from __future__ import annotations

import math
from numbers import Real
from typing import Callable, Iterable, Mapping


def _number(value: object, name: str, *, positive: bool = False) -> None:
    if (isinstance(value, bool) or not isinstance(value, Real)
            or not math.isfinite(value) or value < 0 or (positive and value == 0)):
        raise ValueError(f"{name} must be finite and {'positive' if positive else 'nonnegative'}")


def _words(records: Iterable[Mapping]) -> list[dict]:
    words = []
    previous_start = previous_end = 0
    for index, record in enumerate(records):
        word = dict(record)
        if not isinstance(word.get("text"), str) or not word["text"].strip():
            raise ValueError(f"Word {index} needs nonempty text")
        start, end = word.get("start"), word.get("end")
        _number(start, f"Word {index} start")
        _number(end, f"Word {index} end")
        if end < start:
            raise ValueError(f"Word {index} has a reversed span")
        if start < previous_start or end < previous_end:
            raise ValueError(f"Word {index} clocks must be ordered by start and end")
        previous_start, previous_end = start, end
        words.append(word)
    return words


def _sources(word: Mapping) -> list:
    if "source_phrases" in word:
        return list(word["source_phrases"])
    value = word.get("source_phrase_id")
    return [] if value is None else [value]


def _unique(values: Iterable) -> list:
    result = []
    for value in values:
        if value not in result:
            result.append(value)
    return result


def merge_lexical_units(records: Iterable[Mapping]) -> list[dict]:
    """Join ASR fragments without leading whitespace to their preceding token.

    For example, ``" pre"`` and ``"-existing"`` become ``"pre-existing"``.
    Leading whitespace marks a new lexical unit. All tokens, including zero-width
    points, contribute text; merged units retain their first start and last end.
    """
    result = []
    for word in _words(records):
        value = word["text"]
        if result and not value[0].isspace():
            previous = result[-1]
            previous["text"] += value.rstrip()
            previous["end"] = word["end"]
            previous["source_phrases"] = _unique([*_sources(previous), *_sources(word)])
        else:
            result.append(dict(word, text=value.strip()))
    return result


def wrap_lines(text: str, *, measure: Callable[[str], float],
               max_line_width: float, max_lines: int = 2) -> list[str]:
    """Greedily wrap complete words; raise ValueError if any text cannot fit.

    Width is measured by the caller's deterministic function, usually the exact
    rendering font. Whitespace is normalized to single spaces. Empty text gives
    no lines. Long words are never cut or truncated.
    """
    _number(max_line_width, "max_line_width", positive=True)
    if type(max_lines) is not int or max_lines < 1:
        raise ValueError("max_lines must be a positive integer")
    if not isinstance(text, str):
        raise ValueError("text must be a string")
    rows, row = [], ""
    for word in text.split():
        trial = f"{row} {word}".strip()
        width = measure(trial)
        _number(width, "Measured width")
        if width > max_line_width and row:
            rows.append(row)
            row = word
            width = measure(word)
            _number(width, "Measured width")
        else:
            row = trial
        if width > max_line_width:
            raise ValueError(f"Caption word exceeds max_line_width: {word!r}")
    if row:
        rows.append(row)
    if len(rows) > max_lines:
        raise ValueError(f"Caption exceeds max_lines: {text!r}")
    return rows


def group_words(records: Iterable[Mapping], *, measure: Callable[[str], float],
                max_line_width: float, max_lines: int = 2, target_words: int = 8,
                min_words: int = 4, max_words: int = 10,
                gap_seconds: float = 0.8) -> list[dict]:
    """Balance lexical words within sentences and measured pauses.

    Supply already merged lexical units. A gap strictly greater than gap_seconds
    or a final period, question mark or exclamation mark ends a sentence. Closing
    quotes/brackets after punctuation are accepted. target_words and min_words
    are preferences; max_words and measured line fit are hard limits. Short tails
    are allowed. Every cue uses its first word's start and last word's end.

    Starts and ends must each be nondecreasing; zero-width points and overlapping
    spans are accepted. Overlaps are preserved, even across cue boundaries. A
    zero-width-only sentence or any other infeasible partition raises ValueError
    rather than dropping text, crossing a sentence/pause, or inventing duration.
    """
    for name, value in (("target_words", target_words), ("min_words", min_words),
                        ("max_words", max_words)):
        if type(value) is not int or value < 1:
            raise ValueError(f"{name} must be a positive integer")
    if min_words > target_words or target_words > max_words:
        raise ValueError("Require min_words <= target_words <= max_words")
    _number(gap_seconds, "gap_seconds")
    # Validate layout even for an empty sequence.
    wrap_lines("", measure=measure, max_line_width=max_line_width, max_lines=max_lines)
    words = _words(records)
    sentences, sentence = [], []
    for word in words:
        if sentence and word["start"] - sentence[-1]["end"] > gap_seconds:
            sentences.append(sentence)
            sentence = []
        sentence.append(word)
        if word["text"].rstrip().rstrip('"\'”’)]}').endswith((".", "?", "!")):
            sentences.append(sentence)
            sentence = []
    if sentence:
        sentences.append(sentence)

    result = []
    for sentence in sentences:
        if all(word["start"] == word["end"] for word in sentence):
            text = " ".join(word["text"].strip() for word in sentence)
            raise ValueError(f"Caption needs positive measured spans, not only zero-width points: {text!r}")
        count = len(sentence)
        costs, choices = {count: 0}, {}
        for begin in range(count - 1, -1, -1):
            for end in range(begin + 1, min(count, begin + max_words) + 1):
                if end not in costs or sentence[end - 1]["end"] <= sentence[begin]["start"]:
                    continue
                text = " ".join(word["text"].strip() for word in sentence[begin:end])
                try:
                    wrap_lines(text, measure=measure, max_line_width=max_line_width,
                               max_lines=max_lines)
                except ValueError:
                    continue
                length = end - begin
                penalty = (length - target_words) ** 2
                if length < min_words and count >= min_words:
                    penalty += 80
                score = costs[end] + penalty
                if begin not in costs or score < costs[begin]:
                    costs[begin], choices[begin] = score, end
        if 0 not in choices:
            text = " ".join(word["text"].strip() for word in sentence)
            raise ValueError(f"Cannot fit caption with positive measured spans and layout limits: {text!r}")
        begin = 0
        while begin < count:
            end = choices[begin]
            selected = sentence[begin:end]
            result.append({
                "start": selected[0]["start"], "end": selected[-1]["end"],
                "text": " ".join(word["text"].strip() for word in selected),
                "source_phrases": _unique(source for word in selected for source in _sources(word)),
                "timing": "machine estimate; zero-width words retained in group text",
            })
            begin = end
    return result
