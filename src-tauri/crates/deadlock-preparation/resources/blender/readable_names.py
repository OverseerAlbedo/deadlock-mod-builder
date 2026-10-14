"""Latin spellings of non-English names, so prepared material names stay readable
(`头发` becomes `toufa`, `スカート` `sukaato`, `머리` `meori`) instead of a row of
underscores. Chinese characters (also Japanese kanji) use their Mandarin reading
without tones from the Unicode Unihan data in cjk_readings.txt; kana are spelled
in Hepburn romaji and Hangul syllables by their Unicode names; accented and other
letters lose their marks. ASCII text is returned unchanged, so names that were
already readable keep exactly the prepared name they had.
"""
from pathlib import Path
import unicodedata

_READINGS = None
# Unicode kana names use Kunrei spellings; people read Hepburn.
_HEPBURN = {'si': 'shi', 'ti': 'chi', 'tu': 'tsu', 'hu': 'fu', 'zi': 'ji', 'di': 'ji', 'du': 'zu'}


def cjk_readings():
    """Character to Mandarin reading, read once from cjk_readings.txt."""
    global _READINGS
    if _READINGS is None:
        _READINGS = {}
        try:
            text = (Path(__file__).parent / 'cjk_readings.txt').read_text(encoding='utf-8')
        except OSError:
            text = ''
        for line in text.splitlines():
            if line and not line.startswith('#') and '\t' in line:
                reading, characters = line.split('\t', 1)
                for character in characters:
                    _READINGS[character] = reading
    return _READINGS


def _kana(character):
    """(syllable, kind) of a kana: kind is 'small', 'long', 'double' or ''."""
    if character in 'ーｰ':
        return '', 'long'
    name = unicodedata.name(character, '')
    if not name.startswith(('HIRAGANA LETTER ', 'KATAKANA LETTER ')):
        return None
    words = name.split(' ')[2:]
    small = words[0] == 'SMALL'
    syllable = words[-1].lower()
    if small and syllable in ('tu', 'tsu'):
        return '', 'double'
    return _HEPBURN.get(syllable, syllable), 'small' if small else ''


def _latin(character):
    """The letter without its accent marks, when that is ASCII (é -> e)."""
    plain = ''.join(c for c in unicodedata.normalize('NFKD', character) if not unicodedata.combining(c))
    return plain if plain.isascii() else None


def _script(character):
    if _latin(character) is not None:
        return 'ascii'
    if character in cjk_readings():
        return 'han'
    if _kana(character) is not None:
        return 'kana'
    if unicodedata.name(character, '').startswith('HANGUL SYLLABLE '):
        return 'hangul'
    return 'other'


def _spell(character, script):
    if script == 'han':
        return cjk_readings()[character]
    if script == 'hangul':
        return unicodedata.name(character).split(' ')[-1].lower()
    name = unicodedata.name(character, '')
    if ' LETTER ' in name:
        return name.split(' ')[-1].lower()
    return '_'


def _kana_run(run):
    out = []
    double = False
    for character in run:
        syllable, kind = _kana(character)
        if kind == 'long':
            vowel = next((c for c in reversed(out[-1]) if c in 'aeiou'), '') if out else ''
            if out:
                out[-1] += vowel
        elif kind == 'double':
            double = True
        elif kind == 'small' and out and syllable in ('ya', 'yu', 'yo'):
            # き+ゃ = kya, し+ゃ = sha
            last = out[-1]
            out[-1] = last[:-1] + syllable[1:] if last in ('shi', 'chi', 'ji') else last[:-1] + syllable
        elif kind == 'small' and out and syllable in 'aeiou':
            out[-1] = out[-1][:-1] + syllable  # フ+ィ = fi
        else:
            if double and syllable and syllable[0] not in 'aeiou':
                syllable = ('t' if syllable.startswith('ch') else syllable[0]) + syllable
            double = False
            out.append(syllable)
    return ''.join(out)


def readable(name):
    """Name with non-Latin letters spelled in Latin letters; a different script
    next to a letter or digit is set apart with an underscore."""
    if name.isascii():
        return name
    name = unicodedata.normalize('NFKC', name)
    pieces = []  # (script, text)
    for character in name:
        script = _script(character)
        if pieces and pieces[-1][0] == script and script in ('ascii', 'kana'):
            pieces[-1] = (script, pieces[-1][1] + character)
        else:
            pieces.append((script, character))
    out = ''
    previous = None
    for script, text in pieces:
        spelled = _kana_run(text) if script == 'kana' else ''.join(map(_latin, text)) if script == 'ascii' else _spell(text, script)
        if previous is not None and previous != script and out[-1:].isalnum() and spelled[:1].isalnum():
            out += '_'
        out += spelled
        previous = script
    return out
