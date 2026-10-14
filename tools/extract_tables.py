#!/usr/bin/env python3
"""Generate src/tables.rs from the constant tables of the ITU-T G.723.1 reference software.

Usage: python3 -I tools/extract_tables.py <path to the extracted Software/ directory>

Only constant array initialisers are read. Numeric values are copied unchanged; nothing else
from the reference software is used. Floating-point tables come from the floating-point
reference (Annex B); the channel coding tables come from the Annex C reference.
"""

import ast
import glob
import os
import re
import sys

MAX_WIDTH = 100
ARRAY_WIDTH = 60
SHORT_ELEMENT = 10
INDENT = 4

HEADER = (
    "//! Constant tables of ITU-T G.723.1, extracted from the ITU-T reference software by "
    "tools/extract_tables.py.\n"
)
ALLOW = "#[allow(clippy::excessive_precision, clippy::unreadable_literal)]"

# Source locations: key -> (directory name prefix, file name).
SOURCES = {
    "float_tab": ("G.723.1-Annex-B-Software", "TAB2.C"),
    "float_vad": ("G.723.1-Annex-B-Software", "VAD2.C"),
    "float_lsp": ("G.723.1-Annex-B-Software", "LSP2.C"),
    "fixed_main": ("G.723.1-Annex-A-Software", "LBCCODEC.C"),
    "chan_enc": ("G.723.1-Annex-C-Software", "CHAN_ENC.C"),
    "chan_dec": ("G.723.1-Annex-C-Software", "CHAN_DEC.C"),
    "chan_map": ("G.723.1-Annex-C-Software", "CHAN_MAP.C"),
    "chan_speech": ("G.723.1-Annex-C-Software", "DECOD.C"),
}

# Each table: (Rust name, source key, name in the source, kind, dimensions, clause, description).
# kind is "float", "int" or "hex" (integers printed in hexadecimal).
# An optional eighth field selects the n-th declaration of that name in the source file.
# An optional ninth field lists (source key, name) copies that must hold identical values.
TABLES = [
    ("HAMMING_WINDOW", "float_tab", "HammingWindowTable", "float", (180,),
     "2.4", "Hamming window applied to the 180 analysis samples of each subframe."),
    ("BINOMIAL_WINDOW", "float_tab", "BinomialWindowTable", "float", (10,),
     "2.4", "Binomial window multiplying autocorrelation lags 1 to 10."),
    ("BANDWIDTH_EXPANSION_FACTORS", "float_tab", "BandExpTable", "float", (10,),
     "2.5", "Bandwidth expansion factors 0.994^(j+1) for LPC coefficients j+1."),
    ("LSP_COSINE_TABLE", "float_tab", "CosineTable", "float", (512,),
     "2.5", "One period of a cosine sampled at 512 points, for conversion between LPC and LSP."),
    ("LSP_DC_VECTOR", "float_tab", "LspDcTable", "float", (10,),
     "2.5", "Long-term mean LSP vector pDC removed before LSP prediction and quantization."),
    ("LSP_SUBVECTOR_LAYOUT", "float_tab", "BandInfoTable", "int", (3, 2),
     "2.5", "Start position and length of the three LSP sub-vectors."),
    ("LSP_BAND0_CODEBOOK", "float_tab", "Band0Tb8", "float", (256, 3),
     "2.5", "Codebook of the first LSP sub-vector (8 bits, 3 dimensions)."),
    ("LSP_BAND1_CODEBOOK", "float_tab", "Band1Tb8", "float", (256, 3),
     "2.5", "Codebook of the second LSP sub-vector (8 bits, 3 dimensions)."),
    ("LSP_BAND2_CODEBOOK", "float_tab", "Band2Tb8", "float", (256, 4),
     "2.5", "Codebook of the third LSP sub-vector (8 bits, 4 dimensions)."),
    ("LSP_INTERPOLATION_WEIGHTS", "float_lsp", "Fac", "float", (4,),
     "2.7", "Weight of the current LSP vector in the interpolated vector of each subframe."),
    ("LSP_POLYNOMIAL_SCALES", "float_lsp", "Fac", "float", (3,),
     "2.7", "Scale factors used by the floating-point LSP to LPC polynomial expansion.", 1),
    ("WEIGHTING_FILTER_ZERO_FACTORS", "float_tab", "PerFiltZeroTable", "float", (10,),
     "2.8", "Powers 0.9^(j+1) that scale the numerator of the perceptual weighting filter."),
    ("WEIGHTING_FILTER_POLE_FACTORS", "float_tab", "PerFiltPoleTable", "float", (10,),
     "2.8", "Powers 0.5^(j+1) that scale the denominator of the perceptual weighting filter."),
    ("POSTFILTER_ZERO_FACTORS", "float_tab", "PostFiltZeroTable", "float", (10,),
     "3.8", "Powers 0.65^(j+1) that scale the numerator of the formant postfilter."),
    ("POSTFILTER_POLE_FACTORS", "float_tab", "PostFiltPoleTable", "float", (10,),
     "3.8", "Powers 0.75^(j+1) that scale the denominator of the formant postfilter."),
    ("PITCH_POSTFILTER_GAIN_WEIGHTS", "float_tab", "LpfConstTable", "float", (2,),
     "3.6", "Pitch postfilter gain weighting factor for the high rate and the low rate."),
    ("MPMLQ_PULSE_COUNTS", "float_tab", "Nb_puls", "int", (4,),
     "2.15", "Number of pulses of the high rate excitation in each subframe."),
    ("FIXED_GAIN_LEVELS", "float_tab", "FcbkGainTable", "float", (24,),
     "2.15", "Levels of the 24-step fixed excitation gain quantizer, about 3.2 dB apart."),
    ("MPMLQ_CODEBOOK_SIZES", "float_tab", "MaxPosTable", "int", (4,),
     "2.15", "Number of pulse position codewords of the high rate excitation in each subframe."),
    ("MPMLQ_COMBINATORIAL_TABLE", "float_tab", "CombinatorialTable", "int", (6, 30),
     "2.15", "Binomial coefficients for combinatorial coding of the pulse positions."),
    ("PITCH_GAIN_CODEBOOK_85", "float_tab", "AcbkGainTable085", "float", (85, 20),
     "2.14", "Pitch predictor gain vectors with error-expression terms, 85-entry codebook."),
    ("PITCH_GAIN_CODEBOOK_170", "float_tab", "AcbkGainTable170", "float", (170, 20),
     "2.14", "Pitch predictor gain vectors with error-expression terms, 170-entry codebook."),
    ("ACELP_PITCH_TAP_OFFSETS", "float_tab", "epsi170", "int", (170,),
     "2.16", "Tap offset of the one-tap pitch contribution added to the ACELP vector."),
    ("ACELP_PITCH_TAP_GAINS", "float_tab", "gain170", "float", (170,),
     "2.16", "Gain of the one-tap pitch contribution added to the ACELP vector."),
    ("TAMING_GAIN_BOUNDS_170", "float_tab", "tabgain170", "float", (170,),
     "2.14", "Worst-case single-tap gain of each entry of the 170-entry pitch gain codebook."),
    ("TAMING_GAIN_BOUNDS_85", "float_tab", "tabgain85", "float", (85,),
     "2.14", "Worst-case single-tap gain of each entry of the 85-entry pitch gain codebook."),
    ("PACKED_FRAME_BYTES", "fixed_main", "PackedFrameSize", "int", (2,),
     "4", "Size in octets of a packed active speech frame for the high rate and the low rate."),
    ("VAD_THRESHOLD_SCALE", "float_vad", "ScfTab", "float", (11,),
     "A.2.5", "Voice activity threshold multiplier, sampled at power-of-two noise levels."),
    ("SID_GAIN_ENERGY_SCALES", "float_tab", "fact", "float", (4,),
     "A.4.3", "Scale factors turning accumulated energies into the SID gain quantizer input."),
    ("SID_GAIN_SEGMENT_LIMITS", "float_tab", "bseg", "float", (3,),
     "A.4.3", "Energy level at which each segment of the SID gain quantizer ends."),
    ("SID_GAIN_SEGMENT_BASES", "float_tab", "base", "float", (3,),
     "A.4.3", "Gain index of the first level of each segment of the SID gain quantizer."),
    ("LSP_REORDER_FORWARD_0", "chan_enc", "tabd2_01", "int", (256,),
     "C.2.1.2", "Channel encoder reordering of the first LSP sub-vector index (Table C.1a)."),
    ("LSP_REORDER_FORWARD_1", "chan_enc", "tabd1_01", "int", (256,),
     "C.2.1.2", "Channel encoder reordering of the second LSP sub-vector index (Table C.1b)."),
    ("LSP_REORDER_FORWARD_2", "chan_enc", "tabd0_01", "int", (256,),
     "C.2.1.2", "Channel encoder reordering of the third LSP sub-vector index (Table C.1c)."),
    ("LSP_REORDER_INVERSE_0", "chan_dec", "tabd2_10", "int", (256,),
     "C.3.2.2", "Channel decoder inverse reordering of the first LSP sub-vector index."),
    ("LSP_REORDER_INVERSE_1", "chan_dec", "tabd1_10", "int", (256,),
     "C.3.2.2", "Channel decoder inverse reordering of the second LSP sub-vector index."),
    ("LSP_REORDER_INVERSE_2", "chan_dec", "tabd0_10", "int", (256,),
     "C.3.2.2", "Channel decoder inverse reordering of the third LSP sub-vector index."),
    ("BIT_ORDER_HIGH_RATE", "chan_enc", "BitOrder63", "int", (25, 8),
     "C.2.2", "Position in the ordered bitstream of each high rate bit (Table C.3a).",
     0, [("chan_dec", "BitOrder63")]),
    ("BIT_ORDER_LOW_RATE", "chan_enc", "BitOrder53", "int", (21, 8),
     "C.2.2", "Position in the ordered bitstream of each low rate bit (Table C.3b).",
     0, [("chan_dec", "BitOrder53")]),
    ("BIT_ORDER_SID", "chan_enc", "BitOrderSID", "int", (4, 8),
     "C.2.3", "Position in the ordered bitstream of each SID frame bit (Table C.3c).",
     0, [("chan_dec", "BitOrderSID")]),
    ("CONVOLUTIONAL_PUNCTURING", "chan_enc", "PuncturTable", "hex", (25, 3),
     "C.2.5", "Puncturing patterns per channel rate for the three mother code outputs.",
     0, [("chan_dec", "PuncturTable")]),
    ("CLASS_SIZES_HIGH_RATE", "chan_enc", "BitClass63", "int", (5,),
     "C.2.5.1.1", "Bits per protection class c[i] at the high rate (Table C.5a).",
     0, [("chan_dec", "BitClass63")]),
    ("CLASS_SIZES_LOW_RATE", "chan_enc", "BitClass53", "int", (5,),
     "C.2.5.1.1", "Bits per protection class c[i] at the low rate (Table C.5a).",
     0, [("chan_dec", "BitClass53")]),
    ("CLASS_SIZES_SID", "chan_enc", "BitClassSID", "int", (5,),
     "C.2.5.1.2", "Bits per protection class c[i] for SID frames (Table C.5b).",
     0, [("chan_dec", "BitClassSID")]),
    ("CLASS_WEIGHTS_HIGH_RATE", "chan_enc", "Weight63", "int", (5,),
     "C.2.5.1.1", "Negated weighting factors w[i], high rate, scaled by 32768 (Table C.5a).",
     0, [("chan_dec", "Weight63")]),
    ("CLASS_WEIGHTS_LOW_RATE", "chan_enc", "Weight53", "int", (5,),
     "C.2.5.1.1", "Negated weighting factors w[i], low rate, scaled by 32768 (Table C.5a).",
     0, [("chan_dec", "Weight53")]),
    ("CLASS_WEIGHTS_SID", "chan_enc", "WeightSID", "int", (5,),
     "C.2.5.1.2", "Negated weighting factors w[i], SID frames, scaled by 32768 (Table C.5b).",
     0, [("chan_dec", "WeightSID")]),
    ("CLASS_RECIPROCALS_HIGH_RATE", "chan_enc", "InvBitClass63", "int", (4,),
     "C.2.5.1", "Reciprocals 2^20 / n of the first four high rate class sizes, without tail bits.",
     0, [("chan_dec", "InvBitClass63")]),
    ("CLASS_RECIPROCALS_LOW_RATE", "chan_enc", "InvBitClass53", "int", (4,),
     "C.2.5.1", "Reciprocals 2^20 / n of the first four low rate class sizes, without tail bits.",
     0, [("chan_dec", "InvBitClass53")]),
    ("CONFIG_BLOCK_CODEWORDS", "chan_enc", "ConfigGenMatrix", "int", (8, 13),
     "C.2.5.4", "Eight codewords of the (13,5) block code that protects the configuration bits.",
     0, [("chan_dec", "ConfigGenMatrix"), ("chan_map", "ConfigGenMatrix")]),
    ("SOFT_DECISION_CODEWORDS", "chan_dec", "code_word", "int", (16, 3),
     "C.3", "Soft values (+8192 for 0, -8192 for 1) of the mother code outputs per 4-bit state."),
    ("CONCEALMENT_LEVEL_INCREASE", "chan_speech", "level_increase", "int", (8,),
     "C.3.5", "Largest allowed excitation level rise, in dB, when muting after channel errors."),
]

FLOAT_RE = re.compile(r"[+-]?(\d+\.\d*|\.\d+|\d+)([eE][+-]?\d+)?")


def fail(message):
    sys.exit("extract_tables.py: " + message)


def strip_comments(text):
    text = re.sub(r"/\*.*?\*/", " ", text, flags=re.S)
    return re.sub(r"//[^\n]*", " ", text)


def find_source(software, key):
    prefix, name = SOURCES[key]
    dirs = sorted(glob.glob(os.path.join(software, prefix + "*")))
    if not dirs:
        fail("no directory %s* under %s" % (prefix, software))
    for directory in dirs:
        for sub in (directory, os.path.join(directory, "src")):
            if not os.path.isdir(sub):
                continue
            for entry in os.listdir(sub):
                if entry.lower() == name.lower():
                    return os.path.join(sub, entry)
    fail("file %s not found for %s" % (name, key))


def load_sources(software):
    sources = {}
    for key in SOURCES:
        with open(find_source(software, key), "r", encoding="latin-1") as handle:
            text = strip_comments(handle.read())
        defines = {}
        for match in re.finditer(r"^[ \t]*#[ \t]*define[ \t]+(\w+)[ \t]+(-?\d+)[ \t]*$", text, re.M):
            defines[match.group(1)] = int(match.group(2))
        sources[key] = (text, defines)
    return sources


def initialiser_tokens(text, name, nth):
    pattern = re.compile(r"(?<![\w.])" + re.escape(name) + r"\s*(?:\[[^\]]*\]\s*)+=\s*\{")
    matches = list(pattern.finditer(text))
    if len(matches) <= nth:
        fail("declaration %d of %s not found" % (nth, name))
    start = matches[nth].end() - 1
    depth = 0
    for position in range(start, len(text)):
        if text[position] == "{":
            depth += 1
        elif text[position] == "}":
            depth -= 1
            if depth == 0:
                body = text[start + 1:position]
                break
    else:
        fail("unterminated initialiser of " + name)
    tokens = [token.strip() for token in re.split(r"[,{}]", body)]
    return [token for token in tokens if token]


def evaluate_integer(token, defines):
    expression = re.sub(r"\b(0[xX][0-9a-fA-F]+|\d+)[uUlL]+\b", r"\1", token)

    def walk(node):
        if isinstance(node, ast.Expression):
            return walk(node.body)
        if isinstance(node, ast.Constant) and isinstance(node.value, int):
            return node.value
        if isinstance(node, ast.Name) and node.id in defines:
            return defines[node.id]
        if isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.USub):
            return -walk(node.operand)
        if isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.UAdd):
            return walk(node.operand)
        if isinstance(node, ast.BinOp):
            left, right = walk(node.left), walk(node.right)
            if isinstance(node.op, ast.Add):
                return left + right
            if isinstance(node.op, ast.Sub):
                return left - right
            if isinstance(node.op, ast.Mult):
                return left * right
            if isinstance(node.op, ast.Div):
                quotient = abs(left) // abs(right)
                return quotient if (left < 0) == (right < 0) else -quotient
        fail("cannot evaluate integer expression " + repr(token))

    try:
        return walk(ast.parse(expression, mode="eval"))
    except SyntaxError:
        fail("cannot parse integer expression " + repr(token))


def float_literal(token):
    text = re.sub(r"\(\s*(?:F|FLOAT)\s*\)", "", token).strip()
    if not FLOAT_RE.fullmatch(text):
        fail("not a floating-point literal: " + repr(token))
    text = text.lstrip("+")
    if text.endswith("."):
        text += "0"
    if re.fullmatch(r"-?\d+", text):
        text += ".0"
    return text


def reshape(values, dims):
    if len(dims) == 1:
        return list(values)
    step = len(values) // dims[0]
    return [reshape(values[i * step:(i + 1) * step], dims[1:]) for i in range(dims[0])]


def integer_type(values):
    low, high = min(values), max(values)
    if low >= 0:
        for name, limit in (("u8", 2**8), ("u16", 2**16), ("u32", 2**32)):
            if high < limit:
                return name
    else:
        for name, limit in (("i8", 2**7), ("i16", 2**15), ("i32", 2**31)):
            if low >= -limit and high < limit:
                return name
    fail("integer range too wide")


def read_table(sources, key, name, kind, dims, nth):
    text, defines = sources[key]
    tokens = initialiser_tokens(text, name, nth)
    count = 1
    for dim in dims:
        count *= dim
    if len(tokens) != count:
        fail("%s has %d values, expected %d" % (name, len(tokens), count))
    if kind == "float":
        return [float_literal(token) for token in tokens], None
    numbers = [evaluate_integer(token, defines) for token in tokens]
    return numbers, integer_type(numbers)


def one_line(node):
    if isinstance(node[0], str):
        return "[" + ", ".join(node) + "]"
    return "[" + ", ".join(one_line(child) for child in node) + "]"


def render(node, indent, column, trailer):
    flat = one_line(node)
    if len(flat) - 2 <= ARRAY_WIDTH and column + len(flat) + trailer <= MAX_WIDTH:
        return flat
    inner = indent + INDENT
    lines = ["["]
    if isinstance(node[0], str):
        if all(len(item) <= SHORT_ELEMENT for item in node):
            capacity = MAX_WIDTH - inner - 1
            line = ""
            for item in node:
                cell = item + ","
                if line and len(line) + 1 + len(cell) > capacity:
                    lines.append(" " * inner + line)
                    line = ""
                line = line + " " + cell if line else cell
            lines.append(" " * inner + line)
        else:
            lines.extend(" " * inner + item + "," for item in node)
    else:
        for child in node:
            lines.append(" " * inner + render(child, inner, inner, 1) + ",")
    lines.append(" " * indent + "]")
    return "\n".join(lines)


def format_values(values, dims, kind, radix_hex):
    if kind == "float":
        texts = values
    elif radix_hex:
        texts = ["0x%03x" % value for value in values]
    else:
        texts = [str(value) for value in values]
    return reshape(texts, dims)


def main():
    if len(sys.argv) != 2:
        fail("usage: extract_tables.py <Software directory>")
    software = sys.argv[1]
    if not os.path.isdir(software):
        fail("not a directory: " + software)
    sources = load_sources(software)
    output = [HEADER]
    names = set()
    for entry in TABLES:
        rust_name, key, name, kind, dims, clause, description = entry[:7]
        nth = entry[7] if len(entry) > 7 else 0
        copies = entry[8] if len(entry) > 8 else []
        if rust_name in names:
            fail("duplicate Rust name " + rust_name)
        names.add(rust_name)
        values, int_type = read_table(sources, key, name, kind, dims, nth)
        for other_key, other_name in copies:
            other_values, _ = read_table(sources, other_key, other_name, kind, dims, 0)
            if other_values != values:
                fail("copies of %s differ between sources" % rust_name)
        element = "f64" if kind == "float" else int_type
        declared = element
        for dim in reversed(dims):
            declared = "[%s; %d]" % (declared, dim)
        nested = format_values(values, dims, kind, kind == "hex")
        prefix = "pub static %s: %s = " % (rust_name, declared)
        body = render(nested, 0, len(prefix), 1)
        doc = "/// Clause %s: %s" % (clause, description)
        if len(doc) > MAX_WIDTH:
            fail("documentation line too long for " + rust_name)
        output.append("\n%s\n%s\n%s%s;\n" % (doc, ALLOW, prefix, body))
    os.makedirs("src", exist_ok=True)
    with open(os.path.join("src", "tables.rs"), "w", encoding="utf-8", newline="\n") as handle:
        handle.write("".join(output))


if __name__ == "__main__":
    main()
