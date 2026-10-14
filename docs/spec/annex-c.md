# Annex C: scalable channel coding

This document specifies the channel codec of ITU-T Rec. G.723.1 Annex C closely enough to reproduce the Annex C test sequences bit for bit. The sequences are `chanbit0.tx`, `chanbit1.tx` and `chanbit2.tx`. They are produced from `infobit.tco` with `bitrate0.cfg` or `bitrate1.cfg` and `chanrate.req`, and `chanbit2.tx` is produced in the special test mode of section 1.5.

Clause numbers refer to Annex C of the Recommendation. Table names in capitals (for example `CLASS_SIZES_HIGH_RATE`) are the tables of `docs/spec/tables.md`, with values in `src/tables.rs`. Where the Recommendation text and the reference software differ, the software governs (clause C.4 says so), and the difference is stated in the section concerned and collected in section 13.

## 0. Conventions

### 0.1 What is mandatory

The encoder, as a whole, is mandatory. It is what the three test sequences check. The following parts of the decoder are also mandatory, because they invert encoder operations or must agree with the encoder: the rate allocation (section 5), the bit ordering, the CRC check, the end of trellis exchange, the gain compression and the inverse LPC reordering.

The remaining decoder parts are implementation examples. The reference software says so in its read-me file, and it does not include a soft output Viterbi algorithm. These parts are: the configuration bit decision and the mode change acceptance rule (11.3, 11.4), the Viterbi decoder (11.6), the soft bad frame logic (11.8), the substitution values after forbidden indices (11.10) and the concealment of section 12. The text of C.3.6 sets a minimum performance for the BFI and EFI flags. It does not set an algorithm.

### 0.2 Notation

- floor(x) is the largest integer not above x. "Shift right by n" of a non-negative or negative integer is floor(x / 2^n).
- All arithmetic below is exact integer arithmetic unless a step says otherwise. Where the reference saturates to 16 bits or 32 bits, the step says so. The values of the test sequences never reach a saturation limit in the allocation arithmetic of section 5, and the step notes this.
- Frame types (source mode s): 0 is the high rate (6.3 kbit/s), 1 is the low rate (5.3 kbit/s), 2 is a SID frame, 3 is an untransmitted frame. The two flag bits of every frame are named RATE (frame bit 0) and VAD (frame bit 1), and s = 2 · VAD + RATE.
- Channel configuration c is 0 or 1. It selects the lower or the upper of the two allowed channel rates of the source mode. The configuration index is m = 4 · c + s, a number from 0 to 7. The values s = 3 (m = 3 and m = 7) are reserved.
- Bit numbering inside frames. A frame is a sequence of octets. Octet number n (counting from 1) holds the frame bit indices 8(n−1) to 8(n−1)+7. Inside an octet the bit with weight 2^b has frame bit index 8(n−1)+b. So the tables of the Recommendation, which list each octet with the most significant bit on the left, are read from right to left to obtain increasing frame bit indices. A field named PAR_By is the y-th bit counting from the least significant bit of parameter PAR. A multi-bit field occupies consecutive frame bit indices, and bit B0 of the field has the lowest index. The value of a field is the sum over y of (bit y of the field) · 2^y.
- Soft channel bits: a hard bit 0 is the value +127 and a hard bit 1 is the value −127 (section 11.1).

### 0.3 Fixed constants

| Quantity | High rate (s=0) | Low rate (s=1) | SID (s=2) |
|---|---|---|---|
| Source frame size in octets | 24 | 20 | 4 |
| Information bits N (without flags and unused bits) | 193 | 162 | 30 |
| CRC window W | 44 | 34 | 30 |
| Bits of the channel adapted frame before its unused bits (including the two flag bits) | 195 | 164 | 32 |
| Channel adapted frame size in octets | 25 | 21 | 4 |
| Smallest rate that is accepted (no convolutional code, section 5.3 step 3) | 7034 | 6000 | 1600 |
| Maximum rate with all classes at rate 1/3 | 20634 | 17534 | 4334 |
| Scalar class 0 reciprocal (section 5.3) | 19785 | 24386 | 26887 |

The number of configuration bits is 13 and the number of CRC bits is 5. The scalar class 0 reciprocals are the constants 19785, 24386 and 26887. They approximate 2^20 / 53, 2^20 / 43 and 2^20 / 39 (each rounded up) and are used only in step 7.2 of section 5.3. The tables of `tables.md` do not contain these three scalars, so they are given here.

## 1. Files, framing and the encoder loop (reference software, C.2)

The Recommendation does not describe the files. This section is taken from the reference software.

### 1.1 Information bit file (infobit.tco)

The file is a plain concatenation of G.723.1 packed frames, with no header and no padding.

1. Read one octet. If the file has ended, stop.
2. The frame type is the value of the two least significant bits of this octet (RATE in bit 0, VAD in bit 1). The type fixes the frame length: type 0 has 24 octets, type 1 has 20 octets, type 2 has 4 octets, type 3 has one octet.
3. For types 0 to 2, read the remaining octets (23, 19 or 3). If fewer are available, stop and discard the partial frame.
4. A type 3 frame is an untransmitted frame. No further octets belong to it.

Octets inside a frame follow Tables 5 and 6 of the Recommendation (clause 4) for active frames and Table A.1 (clause A.6) for SID frames. The field positions are listed in section 2.1.

The test file holds 854 frames: 414 of type 0, 352 of type 1 and 88 of type 2, and no type 3 frame. It is 17328 octets long.

### 1.2 Bit rate configuration file (bitrate0.cfg, bitrate1.cfg)

The file is ASCII text. It holds three pairs of integers and comments. The rules of the reference parser are as follows.

1. Read lines. A line is a comment if its first character is a hash sign. Lines may end with a carriage return and line feed or only with a line feed. The reference reads at most 99 characters of a line.
2. To read the first integer of a pair, skip lines until the first line that does not start with a hash sign. Read one decimal integer from the start of this line. The reference accepts leading white space. A blank line is not a comment: it would be taken as the line to read, and its value would be undefined. The supplied files do not contain a blank line before the last value.
3. Read the second integer of the pair from the very next line, without skipping comments, in the same way.
4. The pairs come in this order: first the low rate (source mode 1), then the high rate (source mode 0), then SID (source mode 2). In each pair the first number is the allowed channel rate for channel configuration c = 0 and the second number is the allowed channel rate for c = 1. The values are in bit/s and are stored as 16-bit signed integers.
5. Anything after the sixth integer is ignored (the supplied files end with blank lines).

Values in the supplied files:

| Source mode | bitrate0.cfg (c=0, c=1) | bitrate1.cfg (c=0, c=1) |
|---|---|---|
| low rate | 6000, 17534 | 6000, 11767 |
| high rate | 7034, 20634 | 7034, 13834 |
| SID | 1600, 4334 | 1600, 2967 |

The file `bitrate.cfg` of the package holds other example values and is not used by the test scripts.

### 1.3 Requested rate file (chanrate.req) and fixed rate

The requested channel rate of a frame comes from one of two places.

- If the fourth argument of the program starts with a decimal digit, it is a fixed rate in bit/s (a decimal integer, truncated to 16 bits signed), used for every frame.
- Otherwise it is the name of a binary file. The file holds one 16-bit signed integer per transmitted frame, with the less significant octet first (little endian). The file `chanrate.req` has 854 values and 1708 octets. Its values run from 1600 to 20800 in steps of about 33.3 bit/s (the steps are 34, 33, 33, ...), matching the frame order of `infobit.tco`: the first value (7034) belongs to the first frame, which is a high rate frame.

One value is read for each frame of type 0, 1 or 2, immediately after the frame has been read. No value is read for a type 3 frame. If the file has ended at that moment, the loop stops and the frame is not written.

### 1.4 Channel bit file (chanbit.tx)

Each input frame produces output octets, appended in order.

- A frame of type 0, 1 or 2 produces NB octets, with NB defined in section 9.2. The content is described in sections 2 to 9.
- A frame of type 3 produces exactly two octets, with the values 0xAF and 0x16 in this order. These two octets are a marker that is not a valid configuration code word (section 8). The channel mapping of section 11.1 recognises the marker.

There is no frame header, no length field and no padding between frames.

Expected sizes of the three reference files: `chanbit0.tx` has 20432 octets, `chanbit1.tx` has 29272 octets and `chanbit2.tx` has 38688 octets.

### 1.5 Program options and the test mode

The reference encoder takes the arguments: information bit file, channel bit file, configuration file, rate (fixed value or file), and an optional last argument.

- "/f=n" (letters compared without regard to case) limits the processing to n frames, counting frames of every type. Without it the whole file is processed.
- "-TestSequence" (compared without regard to case on its first 13 characters) switches the encoder to the special test mode of section 5.1. The test mode is not an allowed operating mode. It exists so that all 836 allocations (346 for the low rate, 408 for the high rate, 82 for SID) can be checked with one file.

`chanbit0.tx` is produced with `bitrate0.cfg`, without option. `chanbit1.tx` is produced with `bitrate1.cfg`, without option. `chanbit2.tx` is produced with `bitrate0.cfg` and the test mode.

### 1.6 Per frame procedure of the encoder

For each transmitted frame (type s in 0 to 2):

1. Select the system channel rate R and the channel configuration c from the requested rate (section 5.1). Set m = 4c + s.
2. If m differs from the configuration index of the previous transmitted frame, or if this is the first transmitted frame, compute the rate allocation (section 5). Otherwise keep the allocation of the previous frame. The allocation state is the five class sizes and the five puncturing pattern triplets. The comparison is on m only, never on R. Frames of type 3 are not transmitted and do not change the remembered m.
3. Channel adaptation (section 2): decompress the gain indices and reorder the LPC indices.
4. Ordering (section 3): place the bits in the ordered bitstream.
5. CRC (section 4): insert the five parity bits.
6. End of trellis exchange (section 6).
7. Convolutional coding and puncturing (section 7). Unprotected bits are appended.
8. Prepend the 13 configuration bits (section 8).
9. Pack the bits into NB octets (section 9).

The note in step 2 matters in the test mode. There c is always 0, so m depends on s only. When two successive transmitted frames have the same source mode, the second frame reuses the class sizes of the first one, although its requested rate is different. The frame length in step 9 still follows its own rate. The test file has 61 such frames. They are all in the tail of the file, where only high rate frames remain. An encoder that recomputes the allocation for every frame does not reproduce `chanbit2.tx`.

## 2. Channel adaptation for active speech (C.2.1, C.2.3)

Channel adaptation applies to frames of type 0 and 1. SID frames (type 2) are used as they are: "neither a gain decompression nor a LPC reordering is performed" (C.2.3). A SID frame stays 4 octets long.

### 2.1 Field positions of the packed frames

Frame bit indices as in 0.2. All fields of one frame are listed in order of increasing index.

Packed high rate frame (Table 5), 192 bits:

| Field | First index | Width |
|---|---|---|
| Frame type (RATE at index 0, VAD at index 1) | 0 | 2 |
| LPC | 2 | 24 |
| ACL0 | 26 | 7 |
| ACL1 | 33 | 2 |
| ACL2 | 35 | 7 |
| ACL3 | 42 | 2 |
| GAIN0, GAIN1, GAIN2, GAIN3 | 44, 56, 68, 80 | 12 each |
| GRID0, GRID1, GRID2, GRID3 | 92, 93, 94, 95 | 1 each |
| Unused bit | 96 | 1 |
| MSBPOS | 97 | 13 |
| POS0 | 110 | 16 |
| POS1 | 126 | 14 |
| POS2 | 140 | 16 |
| POS3 | 156 | 14 |
| PSIG0 | 170 | 6 |
| PSIG1 | 176 | 5 |
| PSIG2 | 181 | 6 |
| PSIG3 | 187 | 5 |

Packed low rate frame (Table 6), 160 bits: the fields from the frame type to GRID3 are at the same positions as in the high rate frame. Then:

| Field | First index | Width |
|---|---|---|
| POS0, POS1, POS2, POS3 | 96, 108, 120, 132 | 12 each |
| PSIG0, PSIG1, PSIG2, PSIG3 | 144, 148, 152, 156 | 4 each |

Packed SID frame (Table A.1), 32 bits: frame type at index 0 (2 bits), LPC at index 2 (24 bits), GAIN at index 26 (6 bits).

Field meaning, as far as the channel layer needs it: ACL0 and ACL2 are the 7-bit pitch lag indices of the first and the third subframe. The lag of a pair of subframes is the index plus 18, so the base lag of subframes 0 and 1 is ACL0 + 18 and the base lag of subframes 2 and 3 is ACL2 + 18 (clause 2.18, equation 37). ACL1 and ACL3 are the 2-bit lag differences of the second and fourth subframe. GAINi is the combined 12-bit gain index of subframe i (clauses 2.17 and 2.18). The other fields are copied without change.

### 2.2 Gain index decompression (C.2.1.1)

Each 12-bit gain index GAINi (i = 0 to 3) is split into an 8-bit adaptive codebook gain index AGAINi and a 5-bit fixed codebook gain index FGAINi. One bit more is needed in total for each subframe, as C.2.1.1 says.

For subframe i, let the base lag be L = ACL0 + 18 for i = 0 and 1, and L = ACL2 + 18 for i = 2 and 3.

1. If the frame is a high rate frame and L < 58, the 85-entry pitch gain codebook is in use and bit 11 of GAINi is a separate impulse train bit (clause 2.17, item 5). Then the impulse train bit is T = floor(GAINi / 2048), and the remaining index is G = GAINi − 2048 · T (this is GAINi with bit 11 cleared).
2. Otherwise T = 0 and G = GAINi.
3. AGAINi = floor(G / 24) + 128 · T.
4. FGAINi = G − 24 · floor(G / 24), the remainder of G divided by 24. It is a number from 0 to 23.

The number 24 is the size of the fixed gain table (`FIXED_GAIN_LEVELS` has 24 entries). The text (2.18, equations 39 and 40) writes the threshold condition on "L_i" of the subframe. The reference applies it to the base lag of the pair of subframes, as above. A low rate frame always uses T = 0.

For valid encoder output floor(G / 24) is at most 169 for lags 58 and above (and 84 for lags below 58), so AGAINi always fits in 8 bits and the impulse train bit never overlaps the gain index. The fixed gain index is stored without any remapping (the reference contains a 24-entry identity map for it, kept to allow another mapping).

### 2.3 LPC reordering (C.2.1.2)

The 24-bit field LPC is cut into three 8-bit sub-vector indices:

- e0 = bits LPC_B0 to LPC_B7 (the low octet of the value),
- e1 = bits LPC_B8 to LPC_B15,
- e2 = bits LPC_B16 to LPC_B23.

(The text lists e0 as LPC_B7 to LPC_B3, LPC_B1, LPC_B0, with LPC_B2 missing. This is a misprint.)

Each index is replaced by its table value: R_LPC low octet = `LSP_REORDER_FORWARD_0` indexed by e0, R_LPC middle octet = `LSP_REORDER_FORWARD_1` indexed by e1, R_LPC high octet = `LSP_REORDER_FORWARD_2` indexed by e2. The three results form the new 24-bit field R_LPC = r0 + 256 · r1 + 65536 · r2. Table C.1a, C.1b and C.1c of the text print the same tables as two-dimensional arrays with entry (i, j) at index 16 j + i. The printed Table C.1a has eight entries damaged (see `tables.md`). Use the extracted tables, which are permutations.

### 2.4 The channel adapted frame (Tables C.2a and C.2b)

Build the new frame by writing the fields below in this order, each at the next free frame bit index (starting at 0). Every multi-bit field is written with its bit 0 first.

| Order | Field | Width | Source |
|---|---|---|---|
| 1 | Frame type | 2 | Copied: value 0 for the high rate, 1 for the low rate |
| 2 | R_LPC | 24 | Section 2.3 |
| 3 | ACL0 | 7 | Copied |
| 4 | ACL1 | 2 | Copied |
| 5 | ACL2 | 7 | Copied |
| 6 | ACL3 | 2 | Copied |
| 7 | AGAIN0 to AGAIN3 | 8 each | Section 2.2 |
| 8 | FGAIN0 to FGAIN3 | 5 each | Section 2.2 |
| 9 | GRID0 to GRID3 | 1 each | Copied |
| 10 (high rate) | MSBPOS, POS0, POS1, POS2, POS3, PSIG0, PSIG1, PSIG2, PSIG3 | 13, 16, 14, 16, 14, 6, 5, 6, 5 | Copied |
| 10 (low rate) | POS0 to POS3, then PSIG0 to PSIG3 | 12 each, then 4 each | Copied |

Resulting first indices of the fields. High rate: type 0, R_LPC 2, ACL0 26, ACL1 33, ACL2 35, ACL3 42, AGAIN0 to AGAIN3 at 44, 52, 60, 68, FGAIN0 to FGAIN3 at 76, 81, 86, 91, GRID0 to GRID3 at 96 to 99, MSBPOS 100, POS0 113, POS1 129, POS2 143, POS3 159, PSIG0 173, PSIG1 179, PSIG2 184, PSIG3 190, ending at index 194 (195 bits). Low rate: the same up to GRID3, then POS0 to POS3 at 100, 112, 124, 136, PSIG0 to PSIG3 at 148, 152, 156, 160, ending at index 163 (164 bits).

The frame is stored with the same bit numbering as before, so Tables C.2a and C.2b are obtained. The unused bits "UB" (index 195 to 199 for the high rate, 164 to 167 for the low rate) are zero. The high rate frame has 25 octets and the low rate frame 21 octets.

The unused bit at index 96 of the packed high rate frame is not carried over. The 13-bit field MSBPOS is a plain copy. The reference splits the position information into parts and puts it together again, and the result is identical to a plain copy for every 13-bit value.

## 3. Bit sensitivity ordering (C.2.2, C.2.3)

The tables `BIT_ORDER_HIGH_RATE` (25 rows), `BIT_ORDER_LOW_RATE` (21 rows) and `BIT_ORDER_SID` (4 rows) are the Tables C.3a, C.3b and C.3c. Row ρ (from 0) is octet ρ + 1 of the channel adapted frame (for SID, of the packed SID frame). Column κ (from 0 to 7) is the bit with weight 2^(7−κ) in that octet, which is the frame bit index 8ρ + 7 − κ. This is the order in which the Recommendation prints the octets, from the most significant bit on the left.

Ordering procedure:

1. For every row ρ and column κ with a table entry q that is not −1, set ordered bit i(q) equal to the frame bit with index 8ρ + 7 − κ.
2. Entries of −1 mark the VAD and RATE flags and the unused bits. They are not placed anywhere.

This gives the ordered bitstream i(0) to i(N−1), with N = 193, 162 or 30. i(0) is the most sensitive bit. The entries in the tables are a permutation of 0 to N−1.

Example (first frame of `infobit.tco`, a high rate frame). The first 44 ordered bits are 01010100011111111010000011011001011000010101 and the CRC of section 4 is 1, 0, 0, 1, 0 for p(0) to p(4). The first octets of this input frame in hexadecimal are 54 be f8 08 fe 0a, and of its channel adapted frame 90 9f 3e 0b fe 0a.

## 4. CRC (C.2.4)

The first W bits of the ordered bitstream (W = 44, 34, 30 for high rate, low rate and SID) are protected by five parity bits p(0) to p(4).

Computation. Keep a 5-bit register s, initially 0. For each ordered bit i(k), k = 0 to W−1, in this order:

1. Form the carry c = (i(k) XOR s) AND 1, using the least significant bit of s.
2. Shift s right by one position (the most significant bit becomes 0).
3. If c is 1, replace s by s XOR 18 (binary 10010).

After the last bit, p(4) is the bit 0 of s, p(3) is bit 1 of s, p(2) is bit 2, p(1) is bit 3 and p(0) is bit 4. In other words p(0) is the most significant bit of s.

Equivalent form (checked numerically). Let M(D) be the polynomial with the coefficient i(k) at D^(W−1−k). Divide M(D) · D^5 by the polynomial D^5 + D^3 + 1. Then p(j) is the coefficient of D^j in the remainder. The generator printed in C.2.4 is g(D) = D^5 + D^2 + 1, which is the reciprocal polynomial of D^5 + D^3 + 1. The register algorithm above is what the reference does and it governs.

The bitstream after CRC encoding is (C.2.4):

bs(n) = i(0), ..., i(W−1), p(0), p(1), p(2), p(3), p(4), i(W), ..., i(N−1), for n = 0 to N + 4.

So bs has 198, 167 and 35 elements for the high rate, the low rate and SID. In what follows bs(n) means this sequence.

## 5. Rate allocation (C.2.5.1)

The allocation divides bs into five classes with protection rates 12/r(i), computed from the channel bit budget B of the frame. It runs in the encoder and in the decoder, with the same result.

### 5.1 Selecting the system channel rate R and the configuration c

Let Q be the requested rate (a 16-bit signed integer) and let the allowed rates of the source mode be A0 and A1 (section 1.2).

Normal operation:

1. If Q ≥ A1, then c = 1 and R = A1.
2. Otherwise, if Q ≥ A0, then c = 0 and R = A0.
3. Otherwise the request is too low: stop with an error.

The requested rate is therefore mapped to the next lower allowed rate. R is the channel rate that determines everything that follows, including the length of the output frame.

Test mode (option "-TestSequence"):

1. c = 0 (so m = s) and R = Q. No allowed rate list is consulted.
2. Limit R: for s = 0 it is at most 20634, for s = 1 at most 17534, for s = 2 at most 4334.

The test mode has no lower limit. A rate below the minimum of the mode stops the allocation with an error (see 5.3 step 3).

### 5.2 Available channel bits per frame

B = floor(2 · R · 31458 / 2^21).

This is floor(R · 31458 / 2^20), an integer approximation of R · 30 / 1000. No saturation can occur for 16-bit R. For R = 7034 this gives 211, for R = 20634 it gives 619.

### 5.3 Allocation procedure

Inputs: R (hence B), the source mode s, and the tables of the mode: the initial sizes c(i), the weights w(i), the reciprocals v(i) for i = 0 to 3 and the scalar class 0 reciprocal V0.

| Mode | Initial class sizes c(0) to c(4) | Weights w(0) to w(4) | Reciprocals v(0) to v(3) | V0 | N |
|---|---|---|---|---|---|
| high rate | `CLASS_SIZES_HIGH_RATE` = 49, 44, 46, 51, 12 | `CLASS_WEIGHTS_HIGH_RATE` | `CLASS_RECIPROCALS_HIGH_RATE` | 19785 | 193 |
| low rate | `CLASS_SIZES_LOW_RATE` = 39, 40, 40, 44, 8 | `CLASS_WEIGHTS_LOW_RATE` | `CLASS_RECIPROCALS_LOW_RATE` | 24386 | 162 |
| SID | `CLASS_SIZES_SID` = 39, 0, 0, 0, 0 | `CLASS_WEIGHTS_SID` | none | 26887 | 30 |

Remarks on these tables. Class k of bs, before the allocation changes the sizes, is the stretch of c(k) consecutive elements of bs that follows the stretches of the classes before it. So for the high rate class 0 is bs(0) to bs(48), which is i(0) to i(43) and p(0) to p(4), class 1 is bs(49) to bs(92), class 2 is bs(93) to bs(138), class 3 is bs(139) to bs(185) (47 elements: the table value 51 also counts four tail bits, which are not elements of bs) and class 4 is bs(186) to bs(197). For the low rate the stretches are bs(0) to bs(38), bs(39) to bs(78), bs(79) to bs(118), bs(119) to bs(158) and bs(159) to bs(166). For SID class 0 is bs(0) to bs(34) (35 elements, the table value 39 also counts four tail bits). The low rate table value 44 for class 3 likewise counts four tail bits. The class sizes include the five CRC bits in class 0 (49 = 44 + 5) and four tail bits in class 3 (51 = 47 + 4) or, for SID, in class 0 (39 = 30 + 5 + 4). The weights are stored as the negative of w · 32768, so for example −8520 stands for the weight 0.26 of Table C.5a, and −32768 stands for the weight 1.0. In this document w(i) denotes the stored negative number. The sum of the initial sizes is N + 9: 202, 171 and 39.

The code rates are r(i), the denominators of rate 12 / r(i), with r(i) between 12 and 36. The value r(i) = 12 means class i is not protected. The puncturing patterns of class i are the three numbers of the row r(i) − 12 of `CONVOLUTIONAL_PUNCTURING` (section 7). Whenever a step sets r(i), it also sets the patterns of class i to that row. Initially all five rates are 12 and all patterns are row 0, which is all zero.

Define the function count(c, L), the number of channel bits that the allocation produces if classes 0 to L are protected, as follows. This is used throughout the allocation. The function uses the current sizes c(i) and the current patterns.

1. Set a counter n to 0 and a phase p to 0.
2. For each class k = 0 to L, and inside the class for each of its c(k) information bits in turn: add to n the number of the three patterns of class k whose bit p (weight 2^p) is 1. Then set p to (p + 1) modulo 12. The phase p continues from one class to the next.
3. Add c(L+1) + ... + c(4), the sizes of the unprotected classes.
4. Add 13, the configuration bits.

The result is count(c, L).

Define the function rate-of-class(E, w, c, v, q), the integer computation behind the formula of C.2.5.1 (E is a number of redundancy bits, w a stored weight, c a class size, v a reciprocal, q the rounding constant 0 or 1):

1. S1 = −2 · E · w + 65536 · c. (This is 65536 · (c + E · weight), because w is the negative of weight · 32768. With w = −32768 it is 65536 · (c + E).)
2. S2 = 12 · S1. (The reference computes 8 · S1 + 4 · S1 with 32-bit saturation. Saturation does not occur in the reachable range.)
3. T = floor(S2 / 4096). T is converted to a 16-bit signed number. In the reachable range it is between 0 and about 31000.
4. U = floor(2 · T · v / 2^24). (The product is formed with the 32-bit saturating multiplication of the reference, which does not saturate here.)
5. The result is floor((U + q) / 2).

So the result is 12 · (1 + E · weight / c), computed with q = 1 as the nearest integer (NINT of the text) and with q = 0 as the next lower integer (INT of the text), with the errors caused by the integer reciprocals.

Allocation steps:

1. Set B as in 5.2.
2. Set E = B − (N + 18). (N + 5 CRC bits + 13 configuration bits are the least that is sent.) If E < 0, stop with an error: the rate is too low. (This is the message "Not enough channel bitrate specified".) E is 0 at the smallest allowed rates of the modes, 7034, 6000 and 1600.
3. Case E = 0 (no convolutional code, only the CRC): if s < 2, subtract 4 from c(3), otherwise subtract 4 from c(0). Then add c(0), c(1), c(2) and c(3) to c(4) and set c(0), c(1), c(2) and c(3) to 0. All r(i) = 12 and all patterns are zero. Go to step 9.
4. Compute the limit Lim = 2 · (c(0) + c(1) + c(2) + c(3) + c(4)) + 4, using the initial sizes. This is the Bmax of C.2.5.1 as a number of redundancy bits. It is 408 for the high rate, 346 for the low rate and 82 for SID.
5. Case E ≥ Lim (at least rate 1/3 everywhere): set r(0), r(1), r(2), r(3) to 36 and r(4) to 12, with the patterns of rows 24, 24, 24, 24 and 0. Add c(4) to c(3) and set c(4) to 0. Go to step 9. (If E > Lim, the channel rate is higher than what the code can use. The extra bits are left unused. For the allowed rates of the reference configuration files this never happens.)
6. Otherwise set all r(i) to 12 with zero patterns. Set E4 = E − 4, and if E4 < 0 set E4 = 0. (The four tail bits are sent in any case.)
7. Case "all redundancy in class 0": this case applies if E4 ≤ c(0) + 8 (c(0) still has its initial size here: 49 or 39), or if s = 2.
   1. If s < 2, subtract 4 from c(3). If s = 2, subtract 4 from c(0).
   2. r(0) = rate-of-class(E4, −32768, c(0) + 4, V0, 0). Here c(0) is the size after the subtraction of step 7.1. So the class size argument is 53 (high rate), 43 (low rate) or 39 (SID).
   3. If r(0) = 12, add c(0), c(1), c(2) and c(3) to c(4) and set c(0), c(1), c(2), c(3) to 0. The frame then is unprotected, and the four subtracted bits are lost (they are the tail bits that are not needed).
   4. Otherwise add 4 to c(0). (This puts the four tail bits into class 0.)
   5. Set the patterns of class 0 to the row r(0) − 12.
8. Case "distributed redundancy": this case applies if E4 > c(0) + 8 and s < 2.
   1. r(0) = rate-of-class(E4, w(0), c(0), v(0), 1). If r(0) < 24 set it to 24. If r(0) > 36 set it to 36. Set the patterns of class 0.
   2. Subtract 4 from c(3) and add 4 to c(0). (The tail bits first sit in class 0.)
   3. For i = 1, 2, 3 in this order: compute A = count(c, i − 1). If A < B, then: r(i) = rate-of-class(E4, w(i), c(i), v(i), 1), where c(i) is the current size of class i. Limit r(i) to at least 18 and at most 36, and then to at most r(0). Set the patterns of class i. Subtract 4 from c(i−1) and add 4 to c(i). (The tail bits move with the last protected class.) If A ≥ B, stop this loop at once, leaving the classes i and above with r = 12.
9. Let last be the largest class index i from 1 to 4 with r(i) > 12, and last = 0 if there is none. Compute A = count(c, last).
10. If A > B (too many bits): if s < 2, run procedure "shrink" (below). If s = 2, decrease r(0) by 1, set the patterns of class 0 to the row r(0) − 12, and if r(0) is then 12, subtract 4 from c(0). (The last case does not occur for any allowed rate of SID, 1600 to 4334.)
11. If the allocation went through steps 6 to 8 (that is 0 < E < Lim), and A < B, and r(0) > 12, and s < 2: run procedure "grow" (below). In all other cases nothing is done. (Steps 3 and 5 never grow. In step 7.3 r(0) = 12, and the bits that remain unused are sent as zero.)

Exhaustive evaluation over all integer rates from the minimum to the maximum of each mode shows that the allocation always ends with count(c, last) not above B, that the total of the five sizes equals N + 5 (without protected classes) or N + 9 (with protection), and that no error exit is taken.

Procedure "shrink" (move bits toward the unprotected end until the budget fits). Inputs: the sizes, the patterns, B, the rates r(i) and last.

- If r(3) > 21: repeat the following until it stops. For i = 2, then i = 1: if c(i) = 0, stop the procedure at once. Otherwise move one bit from class i to class i + 1 (c(i) decreases by 1 and c(i+1) increases by 1) and compute A = count(c, last). If A ≤ B, stop the procedure at once. If both i = 2 and i = 1 are done without A ≤ B, start again with i = 2.
- Otherwise (r(3) ≤ 21): repeat the following. If c(last) = 0, stop with an error (this does not occur). Move one bit from class last to class last + 1. Compute A = count(c, last). If A ≤ B, stop.

Procedure "grow" (move bits toward the protected end until the budget is used). Inputs as above.

1. Set the source class src = last + 1 and the destination class dst = last.
2. Repeat: if c(src) = 0, then: if src = last, stop the procedure at once. Otherwise set src = last and dst = last − 1, and if r(src) = r(dst), stop the procedure at once. (This check is made at the moment of the switch only.) Then move one bit from class src to class dst (c(src) decreases by 1, c(dst) increases by 1). Compute A = count(c, last). If A ≥ B, leave the loop.
3. If A > B, run procedure "shrink".

After the allocation the class sizes c(0) to c(4) and the patterns of the five classes are the state of the frame coder. The class sizes include the tail bits in the last protected class.

Differences from the text (see also section 13): the text gives the formulas in terms of B ("channel bits available") as if it were the redundancy count, applies a rate of "1" (unprotected) when r′ is above 12/18, and does not state the tail bit handling or the grow and shrink procedures. The procedure above is what the reference executes.

### 5.4 Check values

The table lists results of the procedure for a few rates. The columns r(0) to r(3) show the row number r − 12 of the pattern table (0 means unprotected). "Octets" is NB of section 9.2.

| Mode | R | B | Sizes c(0) to c(4) | Rows for classes 0 to 3 | Octets |
|---|---|---|---|---|---|
| high | 7034 | 211 | 0, 0, 0, 0, 198 | 0, 0, 0, 0 | 27 |
| high | 8000 | 240 | 59, 38, 46, 47, 12 | 5, 0, 0, 0 | 30 |
| high | 10000 | 300 | 49, 44, 22, 75, 12 | 12, 7, 6, 0 | 38 |
| high | 12000 | 360 | 49, 44, 46, 31, 32 | 12, 11, 9, 8 | 45 |
| high | 13834 | 415 | 49, 44, 46, 54, 9 | 13, 13, 13, 11 | 52 |
| high | 18000 | 540 | 49, 44, 46, 60, 3 | 21, 21, 20, 17 | 68 |
| high | 20634 | 619 | 49, 44, 46, 63, 0 | 24, 24, 24, 24 | 78 |
| low | 6000 | 180 | 0, 0, 0, 0, 167 | 0, 0, 0, 0 | 23 |
| low | 8000 | 240 | 39, 35, 49, 40, 8 | 12, 6, 0, 0 | 30 |
| low | 10000 | 300 | 39, 40, 40, 23, 29 | 12, 11, 8, 7 | 38 |
| low | 11767 | 353 | 39, 40, 65, 27, 0 | 12, 12, 12, 11 | 45 |
| low | 14000 | 420 | 39, 40, 51, 41, 0 | 17, 17, 17, 15 | 53 |
| low | 17534 | 526 | 39, 40, 40, 52, 0 | 24, 24, 24, 24 | 66 |
| SID | 1600 | 48 | 0, 0, 0, 0, 35 | 0, 0, 0, 0 | 6 |
| SID | 2000 | 60 | 39, 0, 0, 0, 0 | 2, 0, 0, 0 | 8 |
| SID | 2500 | 75 | 39, 0, 0, 0, 0 | 7, 0, 0, 0 | 10 |
| SID | 2967 | 89 | 39, 0, 0, 0, 0 | 11, 0, 0, 0 | 12 |
| SID | 4334 | 130 | 39, 0, 0, 0, 0 | 24, 24, 24, 24 | 17 |

(For SID at 4334 the rows of the empty classes 1 to 3 have no effect.)

## 6. End of trellis adaptation (C.2.5.2)

The bits at the end of the protected part of bs have a lower error probability after Viterbi decoding because the trellis ends in a known state. To use this, ten bits near the end are exchanged with ten earlier bits.

Let last be as in step 9 of the allocation (the last protected class, from the final patterns: the largest i from 1 to 4 whose first pattern is not zero, or 0 if there is none). Let Np be c(0) + ... + c(last) − 4. This is the number of protected information bits without the tail bits. If the frame is not protected at all, the sum is c(0) = 0 and Np is −4.

If Np ≥ W + 25 (W as in section 0.3, so the limit is 69, 59 or 55), then with Nlast = Np − 1 exchange, for n = 0 to 9, the elements bs(Nlast − n) and bs(Nlast − 19 + n). The exchange is applied to the whole sequence bs including the CRC bits. In the reference the ten pairs are exchanged one after the other. The pair ranges Nlast − 9 to Nlast and Nlast − 19 to Nlast − 10 do not overlap, so the order does not matter.

The text states the condition as "more than 19 information bits are protected in the classes 1 to 3". The condition above is the same statement: Np minus (W + 5) is at least 20. The exchange is its own inverse, so the decoder applies the same operation after the Viterbi decoding.

## 7. Convolutional encoding and puncturing (C.2.5, C.2.5.3)

### 7.1 Mother code

The mother code has constraint length 5 and three generators:

- generator 0: D^4 + D + 1 (octal 23),
- generator 1: D^4 + D^3 + D^2 + 1 (octal 35),
- generator 2: D^4 + D^2 + D + 1 (octal 27).

Let x(n) be the input bit sequence of the encoder, with x(n) = 0 for n < 0. The output bits for input position n are the sums modulo 2:

- u0(n) = x(n) + x(n−1) + x(n−4),
- u1(n) = x(n) + x(n−2) + x(n−3) + x(n−4),
- u2(n) = x(n) + x(n−1) + x(n−2) + x(n−4).

The register is cleared to zero at the start of every frame. It is not cleared between classes.

### 7.2 Input sequence

This section applies if the allocation left class 0 protected (r(0) > 12). Otherwise there is no coding, see the end of 7.3.

Let K = N + 5, the length of bs, and let n0 = c(last + 1) + ... + c(4), the number of unprotected bits (0 when the last protected class is class 3 and class 4 is empty). The input sequence x of the encoder has K − n0 + 4 elements, which equals c(0) + ... + c(last):

1. x(n) = bs(n) for n = 0 to K − n0 − 1.
2. x(n) = 0 for n = K − n0 to K − n0 + 3. These are the four tail bits. They bring the register back to the all-zero state.

The unprotected bits are bs(K − n0) to bs(K − 1). They are not coded and are sent after the coded bits, unchanged. The first four of them have the same positions in bs as the tail bits have in x. Their values are those of bs, not zero.

### 7.3 Puncturing

The 12 bit patterns of the three generators for rate 12 / (12 + ρ) are in row ρ of `CONVOLUTIONAL_PUNCTURING` (rows 1 to 24 are Table C.4, row 0 is all zero). Column k is the pattern of generator k. In the reference the three patterns of class i are the row r(i) − 12.

Encoding runs over the classes 0 to last in order. A phase p starts at 0 at the start of the frame and increases by 1 (modulo 12) for each input bit, across class boundaries. It is not reset at a class boundary, and it also counts the tail bits. For each input bit x(n), belonging to class k:

1. Compute u0(n), u1(n), u2(n).
2. For generator j = 0, 1, 2 in this order: if bit p (weight 2^p) of the pattern j of class k is 1, append u(j)(n) to the coded output sequence u. Otherwise discard it.
3. Increase p by one modulo 12.

The class of the input bit x(n) is found from the cumulated class sizes: bits 0 to c(0) − 1 belong to class 0, the next c(1) bits to class 1, and so on. The tail bits belong to the last protected class, because its size c(last) includes them. Therefore the patterns of the last protected class also apply to the tail bits.

Call M_P the number of bits in u after the protected part has been coded. After the coded part, append the unprotected bits bs(K − n0) to bs(K − 1) in order. They take the positions M_P to M_All − 1 of u.

If no convolutional code is used (r(0) = 12 after the allocation, so the patterns of class 0 are all zero), the sequence u is just bs(0) to bs(K − 1) and M_All = K.

In all cases M_All = count(c, last) − 13, with count as defined in section 5.3.

## 8. Configuration bits (C.2.5.4)

The 13 configuration bits ucb(0) to ucb(12) encode the configuration index m = 4c + s. The codeword is row m of `CONFIG_BLOCK_CODEWORDS`: ucb(n) = 0 if the entry in column n is +127 and ucb(n) = 1 if it is −127.

Editorial error of the text. C.2.5.4 calls the code a (13,5) block code, but the reference states that it is a (13,3) block code. This agrees with the table (3 information bits, 8 code words, minimum distance 7). Table C.6 shows two source bits (VAD, RATE) and one channel configuration bit "ccc". Only the three bits c, s1 and s0 select the code word.

The table is the normative content. The generator polynomial printed in C.2.5.4 is g(D) = D^10 + D^8 + D^5 + D^2 + D + 1. The rows of the table are shifts of the word 1110110010100 (row 1), which is the coefficient list of 1 + D + D^2 + D^4 + D^5 + D^8 + D^10. That is the printed polynomial with an additional D^4 term. Row 2 is row 1 moved right by one position, and row 4 is row 2 moved right by one position. The other rows are sums modulo 2 of rows 1, 2 and 4 (row 3 = row 1 + row 2, row 5 = row 1 + row 4, row 6 = row 2 + row 4, row 7 = row 1 + row 2 + row 4). Use the table.

The 13 bits are transmitted before the coded information bits.

## 9. Channel frame format (C.2.5.4, Table C.7)

### 9.1 Layout

Let the channel bit sequence be chan(0) to chan(8 · NB − 1):

- chan(0) to chan(12) are ucb(0) to ucb(12),
- chan(13 + j) = u(j) for j = 0 to M_All − 1 (protected coded bits, then unprotected bits),
- all further positions up to 8 · NB − 1 are 0.

Octet number t (from 0) of the output is the sum over b = 0 to 7 of chan(8t + b) · 2^b. So chan(0) is the least significant bit of the first octet. In the notation of Table C.7 the first octet is ucb(7) ... ucb(0) from the most significant to the least significant bit, the second octet is u(2), u(1), u(0), ucb(12), ..., ucb(8), and so on. The unused bits "UB" of the last octet are zero and occupy the most significant positions.

### 9.2 Number of octets NB

NB is computed from the system channel rate R of 5.1 (not from B, and not from M_All) with integer arithmetic:

1. NB = floor(2 · R · 31457 / 2^24).
2. Compute K2 = floor(2 · NB · 17067 / 128). If K2 < R, add 1 to NB.

The rule approximates the ceiling of R · 30 / 8000, the number of octets in 30 ms. For the rates 17067, 17867, 18667, 19467 and 20267 (and every 800 higher, up to 32267) it gives one octet less than that ceiling. Six frames of the test mode run have such a rate after the limits of 5.1 (frames 691, 739, 754, 787, 814 and 838, counting from 1), so `chanbit2.tx` checks the fixed-point rule. In these cases the number of octets is still large enough to hold 13 + M_All bits. The fixed-point rule is what the encoder does and what has to be reproduced.

The rate figure of C.2.5.4 for the bit rate, (M_All + 13) · 1000 / 30, is the rate of the used bits. The actual frame is NB octets long.

## 10. Test procedure

1. Encode `infobit.tco` with `bitrate0.cfg` and `chanrate.req` in normal mode and compare with `chanbit0.tx`.
2. The same with `bitrate1.cfg` and compare with `chanbit1.tx`.
3. Encode with `bitrate0.cfg` and `chanrate.req` in the test mode (5.1) and compare with `chanbit2.tx`. Remember the allocation reuse of 1.6.

All three outputs must be identical, octet for octet, including the length. A byte-identical result of all three confirms the allocation arithmetic, the ordering, the CRC, the trellis exchange, the convolutional code, the puncturing, the configuration code and the packing. The three files also check each of the three modes. Of these, `chanbit2.tx` checks the allocation for 836 combinations of mode and rate.

Debugging aid: in `chanbit0.tx` the first frame is a high rate frame at the requested rate 7034 and has no protection (check value table of 5.4: sizes 0, 0, 0, 0, 198). Its output holds 27 octets. The 13 configuration bits of m = 0 are all zero, and the 198 bits bs(0) to bs(197) follow without coding from chan(13) on. The first four octets are 00 40 c5 bf in hexadecimal, and the whole frame is 00 40 c5 bf 60 d3 50 13 af 43 a6 81 04 84 dd c7 d1 f8 a2 e1 89 f0 3c 9f 92 57 01.

## 11. Channel decoder (C.3)

### 11.0 Overview

The chain of the reference is: encoder, then the "network" (CHAN_MAP, section 11.1), then the channel decoder, then the speech decoder. The decoder reads one fixed-size record per frame. Sections 11.2 to 11.11 follow the order of processing in the decoder.

### 11.1 Soft channel bit file (chanbit.rx), produced by CHAN_MAP (not mandatory)

CHAN_MAP turns the packed hard decisions of `chanbit.tx` into the soft input of the decoder. It simulates an error-free channel.

For each frame, the output record has exactly 684 octets, each a signed 8-bit number.

1. Read two octets. If the file has ended, stop. If they are 0xAF and 0x16, the frame is untransmitted: write 684 zero octets.
2. Otherwise decode the 13 configuration bits (the bits 0 to 7 of the first octet from the least significant, bits 0 to 4 of the second octet; each bit 0 is +127 and each bit 1 is −127) by the maximum correlation rule of 11.3, giving m. Set s = m modulo 4 and c = floor(m / 4). The system channel rate R is the allowed rate of source mode s and channel configuration c from the configuration file.
3. Compute NB from R. The reference uses a floating-point form (floor of R · 30 / 8000, plus 1 if that times 8000 / 30 is below R). For the rates 17067 + 800 k it gives one octet more than the encoder of 9.2. A decoder that wants to read encoder files for these rates must use the rule of 9.2. For the rates of the reference configuration files both rules agree.
4. Read the remaining NB − 2 octets. If the file ends early, stop.
5. For each bit position i = 0 to 8 · NB − 1 (bit i mod 8 of octet floor(i / 8), least significant bit first): output +127 (octet 0x7F) if the bit is 0 and −127 (octet 0x81) if the bit is 1. The remaining positions of the 684 are 0.

The value 0 means "no information", used for bits that were not transmitted. The reference decoder treats an all-zero record as an untransmitted frame.

### 11.2 Decoder record and untransmitted frames

The channel decoder reads 684 signed octets per frame and stops at the end of file. If all 684 are 0, the frame is untransmitted: the speech decoder receives a frame with type 3 and BFI = 0, and the channel decoder is not run. Otherwise sections 11.3 to 11.10 apply. The soft values s(0) to s(683) are the elements of the record. s(0) to s(12) are the configuration bits and s(13 + j) the soft value of u(j).

### 11.3 Decoding of configuration bits (C.3.1; not mandatory)

Compute the 8 correlations d(m′) = sum over n = 0 to 12 of s(n) · e(m′, n), where e(m′, n) is the entry (row m′, column n) of `CONFIG_BLOCK_CODEWORDS` (+127 or −127). Take the largest value, choosing the lowest index m′ when equal (the comparison is "greater than"). This is m, with dmax the value. Then set the entry for m to the lowest representable value (−2147483647) and take dsecond, the largest of the other correlations. Reliability: conf = min(32767, floor((dmax − dsecond) / 8)).

### 11.4 Mode change acceptance (not mandatory)

The decoder keeps an accepted mode, which starts at 0. The mode m of the current frame is accepted if m equals the accepted mode. Otherwise it is accepted only if conf > 1000 and the source mode m modulo 4 is not 3. In that case the accepted mode becomes m. If it is not accepted, the frame is treated as lost: the data of the previous decoded frame is passed on again with BFI = 1 and EFI = 1, and nothing else is done. The reference leaves FII undefined in this case (use 0), and its behaviour is also undefined if there is no previous frame or the previous frame was untransmitted.

When the mode is accepted and m differs from the mode of the previous accepted frame (the decoder keeps the mode of the last allocation, initially none), the rate allocation of section 5 is computed with the allowed rate R of source mode s and configuration c from the configuration file (no request mapping). The mode of the last allocation then becomes m.

### 11.5 Allocation (mandatory)

The class sizes and patterns of the decoder are those of the encoder for the same m and R. The decoder does not need to know the actual requested rate.

### 11.6 Viterbi decoding (not mandatory; the reference gives hard output only)

Soft values are depunctured: for the protected part, the decoder runs over the same input positions n and phases p as the encoder, over classes 0 to last. For each position it forms three input values z0, z1, z2. For generator j, if bit p of the pattern j of class k is 1, then zj = 2 · s(P), where P is the position of the next unread coded bit (starting at 13, increasing by 1 for each used bit). Otherwise zj = 0.

Trellis. There are 16 states. A state stands for the last four input bits: state = x(n) + 2 · x(n−1) + 4 · x(n−2) + 8 · x(n−3) after the step for position n. The state before position n is Q = x(n−1) + 2 x(n−2) + 4 x(n−3) + 8 x(n−4).

Branch metric of the step at position n from the previous state Q with input bit b = x(n): form the index j = 2 · (Q modulo 8) + ((b + floor(Q / 8)) modulo 2). (Q modulo 8 contains x(n−1), x(n−2), x(n−3) and floor(Q / 8) = x(n−4).) The branch metric is the sum over generators k of 2 · SD(j, k) · zk, where SD(j, k) is entry k of row j of `SOFT_DECISION_CODEWORDS` (±8192), after which the sum is shifted right by 16 (floor). It is a signed number between about −191 and +190.

Add the branch metric to the metric of state Q with a 16-bit saturating addition. For each new state, take the larger of the two candidates, preferring the candidate with the previous state's top bit 0 (x(n−4) = 0) in a tie. Record the choice by one bit per state and position: 0 if the candidate with x(n−4) = 0 was taken, 1 otherwise.

Initial metrics: state 0 has metric 0, states 1 to 15 have metric −5000. Metrics are not normalised. Saturation at 32767 can occur in a long error-free frame, and the reference accepts this.

Traceback. Start in state 0 (the tail bits drive the encoder to state 0). For position n from the last position down to 0: the decoded bit is the least significant bit of the current state; if the recorded choice bit for the current state at n is 1, add 16 to the state; then shift the state right by one. The decoded bits for positions 0 to K − n0 − 1 are the information bits. The last four decoded bits are tail bits and are dropped.

Unprotected bits: bit n is 1 if the soft value is negative and 0 otherwise (a value 0 gives 0), taken from s(13 + M_P) onward in order.

If the frame has no protection, the information bits are the hard decisions of s(13) to s(13 + K − 1).

The soft output (a reliability for each information bit) is not part of the reference. The reference sets all soft output values to 127.

### 11.7 End of trellis exchange, CRC and ordering (mandatory)

1. Apply the exchange of section 6 to the decoded sequence bs, together with the soft values in the same way.
2. CRC check. Run the register of section 4 over bs(0) to bs(W − 1). Compare the bits of the register with the received parity bits bs(W) to bs(W + 4): p(4) is bit 0 of the register, ..., p(0) is bit 4. If one of the five bits differs, the CRC flag is set.
3. Remove the parity bits: the ordered bitstream i(k) is bs(k) for k < W and bs(k + 5) for k ≥ W.
4. Inverse ordering: for every table entry q ≠ −1 at row ρ and column κ of the bit order table of the mode, frame bit 8ρ + 7 − κ of the channel adapted frame is i(q). All other bits are 0. Then the frame type (2 bits) is written into frame bits 0 and 1.

### 11.8 Bad frame indication BFI and erroneous frame indication EFI (C.3.3)

BFI signals uncorrected errors inside the CRC window. EFI signals uncorrected errors outside the CRC window, and is only evaluated if bits outside the window are protected. For SID frames only BFI is defined.

Minimum CRC based flag: BFI = 1 if the CRC check of 11.7 fails. The reference improves the decision with the soft output; since the reference soft output is constant (127), the improvement does nothing there. The method is described here for completeness (not mandatory). It is applied only if the first pattern of class 0 is not zero.

State: a history of 20 frame means, all initially 127, and 5 class means from the last frame, all initially 127.

1. Let last and the class sizes c(i) be as in the allocation. Define class sizes without tail bits: b(i) = c(i) for i < last and b(last) = c(last) − 4.
2. Mean of the history: Hm = integer part of the sum of the last 10 history entries divided by 10.
3. Shift the history by one place and put at the end the integer part of the mean of the soft output values of the information bits 0 to W − 1. Call this value Hn.
4. Soft BFI = 1 if Hn < 0.17 · Hm or Hn < 7.
5. Class means: mean(0) is the integer part of the mean of the soft values of the ordered positions W to b(0) − 1 (including the CRC bits), if b(0) > W + 5, otherwise 127. For i = 1 to last: the integer part of the mean over the next b(i) positions if b(i) > 0, otherwise 127.
6. Soft EFI = 1 if any one of the following holds: at least three of the classes 0 to last have mean(i) < integer part of 0.35 · Hn; at least two classes have mean(i) < 20; at least one class has mean(i) < 15; at least three of the classes 1 to last have mean(i) < integer part of 0.35 times the mean of the same class in the previous frame. Then store the means for the next frame (classes 0 to last).
7. If the CRC passed, the soft BFI is 1 and the soft EFI is 0, set the soft BFI to 0 and the soft EFI to 1.

Final flags: BFI = 1 if the CRC failed or the soft BFI is 1; EFI = soft EFI; FII from 11.10.

### 11.9 Gain compression and inverse LPC ordering (C.3.2; mandatory)

Applied to frames of type 0 and 1. The input is the channel adapted frame of 11.7 (section 2.4 layout). The output is the packed frame (section 2.1 layout).

1. Inverse LPC ordering (C.3.2.2): split the 24-bit R_LPC field into three octets as in 2.3 and replace them by `LSP_REORDER_INVERSE_0` of the low octet, `LSP_REORDER_INVERSE_1` of the middle octet, `LSP_REORDER_INVERSE_2` of the high octet.
2. Lags: the lag indices ACL0 and ACL2 are copied (but see 11.10 for forbidden values).
3. Gain compression: for each subframe i, with the base lag L as in 2.2 (after the replacement of 11.10): if the frame is a high rate frame and L < 58, let T be bit 7 of AGAINi and G = AGAINi modulo 128; otherwise T = 0 and G = AGAINi. GAINi = 24 · G + FGAINi, and then, in the first case, plus 2048 · T.
4. Write the packed frame: type, LPC, ACL0, ACL1, ACL2, ACL3, GAIN0 to GAIN3 (12 bits each), the four GRID bits, the unused bit (0) for high rate frames, and the position and sign fields. The fields from the grid onwards are copies.

For SID frames the 4-octet frame of 11.7 is the packed SID frame as it is.

### 11.10 Forbidden indices and the FII flag (C.3.2.1; replacement values not mandatory)

The text says: if a forbidden index occurs in the decoded adaptive codebook gain, in the fixed excitation gain or in the lag, the flag FII is set and the index is replaced by a valid value. The reference rules are:

1. A lag index ACL0 or ACL2 above 123 is forbidden. It is replaced by the last valid lag (the last valid ACL0 for the first, the last valid ACL2 for the second; both start at 0 and are kept from frame to frame). For a replaced ACL0 add 1 to the lag error count, for a replaced ACL2 add 2 to it. The base lag used for the later checks is the replaced one.
2. For each subframe, an adaptive gain index G (7 bits with T taken off in the 85-entry case, otherwise the 8-bit value) is forbidden if G ≥ 85 in the 85-entry case (high rate and base lag below 58) or G ≥ 170 otherwise, or if the lag of the pair was forbidden. Then G and T are replaced by the last valid G and T (initially 0) and the gain error count increases by 1. Valid values update the last valid G and T.
3. A fixed gain index of 24 or more is forbidden. It is replaced by the last valid fixed gain index (initially 0) and the fixed gain error count increases by 1. Valid values update the last valid index.
4. FII = 1 if the sum of the three counts is above 0. For SID frames FII = 0.

After the replacement, the gain compression of 11.9 uses the replaced values.

### 11.11 Interface to the speech decoder (Tables C.8a to C.8c)

The frame handed to the speech decoder is the packed frame with one extra octet appended: octet 25 (high rate), 21 (low rate) or 5 (SID), counted from 1. Bit 0 of this octet is BFI, bit 1 is EFI and bit 2 is FII. The other bits are 0. In Tables C.8a to C.8c the octet is printed as UB, UB, UB, UB, UB, FII, EFI, BFI from the most significant bit down. For an untransmitted frame the speech decoder gets the frame type 3 with BFI = 0. BFI is also passed to the speech decoder as its erasure indication.

## 12. Error concealment (C.3.5; not mandatory)

The text only states that the BFI, EFI and FII flags "shall be used in the speech decoder for error concealment purposes". The reference software contains one possible method as an extension of the speech decoder. It does not change the output for error-free frames. Table: `CONCEALMENT_LEVEL_INCREASE` = 0, 0, 3, 6, 10, 15, 20, 15.

State: the level memory (initially −72), and a flag "last frame suspicious" (initially 0). The method runs for each subframe of an active frame after the excitation has been formed. For an erased frame (BFI = 1 and the frame is regenerated by the interpolation of clause 3.10), it runs on each 60-sample subframe of the regenerated excitation in the same way.

At the first subframe of a frame:

1. Read BFI, EFI and FII from the appended octet.
2. Set the mute permission to 0. If BFI = 0 and (EFI = 1 or FII = 1), set the mute permission to 1. If BFI = 1 and "last frame suspicious" = 1, set the mute permission to 1.
3. If BFI = 0, set "last frame suspicious" to 1 if EFI = 1 or FII = 1, otherwise to 0. If BFI = 1 it is unchanged.

For each subframe with excitation samples e(0) to e(59) (16-bit integers):

1. Energy E = (e(0)^2 + ... + e(59)^2) / (60 · 4096 · 4096), in floating point.
2. Level = integer part (towards zero) of 10 · log10(E) if E > 0, otherwise −72. If the level is below −72, it is −72.
3. If the mute permission is 1: index d = (5 − level memory) / 10 with integer division towards zero, limited to the range 0 to 7. Let excess = level − (level memory + CONCEALMENT_LEVEL_INCREASE(d)). If excess > 0, multiply every sample by 10^(−excess / 20) and truncate towards zero to a 16-bit integer. The level memory is not changed.
4. If the mute permission is 0: the level memory becomes the level.

The reference is a post-processing choice, not a conformance item.

## 13. Differences between the text and the reference

1. C.2.5.4 calls the configuration code (13,5). It is a (13,3) code (the read-me file records this as an editorial error). The generator polynomial printed in C.2.5.4 does not reproduce the table (section 8).
2. C.2.1.2 prints the sub-vector e0 without LPC_B2. Table C.1a is partly damaged in the supplied text (see `tables.md`); the extracted tables are used.
3. C.2.4 prints g(D) = D^5 + D^2 + 1. The reference register computes the remainder of the reciprocal polynomial D^5 + D^3 + 1 with the first bit of the window as the highest power, and p(0) is the coefficient of D^0 (section 4).
4. C.2.5.1: the formulas use B for the available bits and a threshold of 12/18 for "unprotected". The reference uses the redundancy count B − N − 18, subtracts four tail bits, clamps the code rate of classes 1 to 3 to at least 18 (not "unprotected"), caps lower classes at the rate of class 0, and adjusts the result by the shrink and grow procedures. The text does not describe the case selection E = 0, E4 ≤ c(0) + 8 and E ≥ Lim.
5. 2.18 (equations 39 and 40) uses the lag of each subframe for the choice of the 85-entry or 170-entry gain codebook. The reference uses the base lag of the pair of subframes (ACL0 + 18 or ACL2 + 18) for all four subframes.
6. C.2.5.3 says "p is a modulo 12 counter incremented with the information bit counter j". It does not say that the counter starts at 0 for each frame, runs through the tail bits and is not reset at class boundaries; the reference does.
7. The text does not describe the two byte marker 0xAF 0x16 of an untransmitted frame, nor the test mode.
8. The octet count NB of a channel frame (Table C.7) is not given in the text. The fixed-point rule of 9.2 deviates from the ceiling of R · 30 / 8000 for R = 17067 + 800 k.
9. The text of C.3.3 does not say how BFI is derived beyond the CRC; the soft evaluation of 11.8 is the reference's example.
