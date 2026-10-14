# G.723.1 decoder: bitstream, frame flow, postfilter, concealment, output, test formats

This document specifies the decoder of ITU-T Recommendation G.723.1 as implemented by the Annex B floating-point reference (version 5.2F). It covers clause 3 (decoder principles), clause 4 (bitstream packing), the decoder side of clause 2 (2.17 and 2.18), and the file formats and comparison rules of the Annex B test suite. Comfort noise generation is specified in the Annex A document; this document says where the decoder branches to it and which state that branch touches. The LSP inverse quantizer and the LSP to LPC conversion are specified in the encoder analysis document (LSP inverse quantization, LSP interpolation, LSP to LPC conversion); this document gives the decoder-only differences and restates the decoder procedure so that it can be read alone. The fixed codebook vector generation (2.17, 3.5) is specified in the fixed codebook document; this document gives only the interface and the frame-level combination.

Where the Recommendation text and the reference differ, the reference is authoritative, because the conformance tests compare against its output. Such places are marked "The text says ...; the reference ...", and they are collected in section 14.

## 0. Conventions

1. Arithmetic. The reference computes every decoder quantity in IEEE double precision (64 bit). No single-precision rounding is part of the reference behaviour anywhere in the decoder. The one place where a single-precision constant appears is the comparison threshold FLT_MIN, 1.17549435e-38 (the smallest normal single-precision number). It is used as a plain double constant in comparisons. An implementation in f64 must use f64 everywhere, including for tables (which are given to about six significant digits and are used as printed).
2. Integer quantities (lags, indices, counters) are exact integers. The pseudo-random seed is a 16-bit quantity with wrap-around arithmetic as defined in section 10.4.
3. Constants: frame length 240 samples, subframe length 60 samples, 4 subframes per frame, LPC order 10, minimum pitch lag 18, maximum pitch lag 18 + 127 = 145 (the excitation memory length), five-tap pitch predictor, number of gain levels of the fixed codebook gain quantizer 24 (table FIXED_GAIN_LEVELS), maximum erasure counter 3.
4. LSP frequencies are in units of pi/256 radian (15.625 Hz). A distance of 31.25 Hz is 2.0 units and 62.5 Hz is 4.0 units.
5. Subframes are numbered 0 to 3. Two pitch lag values per frame are transmitted. The first, called lag A here, is used by subframes 0 and 1. The second, called lag B, is used by subframes 2 and 3. Lag A and lag B are the values L0 and L2 of the Recommendation text.
6. Excitation memory notation. The decoder keeps the previous 145 excitation samples. Write e(n) for the excitation, with n = 0 to 239 the samples of the current frame and n = -1, -2, ..., -145 the memory (e(-1) is the most recent past sample).
7. Rounding of float to integer, where used, is described at the place of use. A "strict" comparison means "greater than" or "less than" without equality.

## 1. Frame types and the frames of a bitstream file (clause 4, Tables 5 and 6, Annex A.6)

### 1.1 Frame type bits

The first two bits of every frame, in transmission order, are RATEFLAG then VADFLAG. They are read as a 2-bit number (first bit least significant) called the frame type code:

| Code | VADFLAG | RATEFLAG | Meaning | Octets in the file |
|---|---|---|---|---|
| 0 | 0 | 0 | Active frame, high rate (6.3 kbit/s) | 24 |
| 1 | 0 | 1 | Active frame, low rate (5.3 kbit/s) | 20 |
| 2 | 1 | 0 | SID (silence insertion descriptor) frame | 4 |
| 3 | 1 | 1 | Untransmitted frame | 1 |

The text calls the combination of both flags set "reserved for future use". The reference uses it as the "untransmitted" frame: a 1-octet frame whose low two bits are 1 and whose other six bits are 0 when written. It is how the reference represents a frame that a silence-compression encoder decided not to send, so that the decoder can be driven frame by frame. When read, only the two low bits of that octet are looked at. The sizes 24 and 20 are the table PACKED_FRAME_BYTES (index 0 high rate, index 1 low rate). The sizes 4 and 1 have no table.

### 1.2 Finding the length of each frame in a file

A bitstream file is the concatenation of frames with no header, no padding and no separators. The reader reads one octet and looks at its two low bits (the frame type code). The code gives the total frame length from the table above, and the reader reads the remaining length minus 1 octets. The decoder processes frames until the single-octet read at a frame boundary fails (end of file). Every frame, including SID and untransmitted frames, produces 240 output samples.

The reference does not check that the last frame is complete. If the file ends inside a frame, the missing octets keep whatever values the frame buffer held from the previous frame. The conformance files contain only whole frames, so this case need not be reproduced.

The frame length depends only on the two low bits of the first octet, never on the erasure indication. An erased frame still occupies its normal length in the file.

### 1.3 Serial bit order

Bits are numbered in transmission order, k = 0 for the first. Bit k of the frame is bit number (k modulo 8) of octet number (k divided by 8, integer division), where octet 0 is the first octet of the frame and bit number 0 is the least significant bit of the octet. This reproduces the statement of clause 4 that octets are listed in transmission order and that within each octet the most significant bit is on the left in Tables 5 and 6. A multi-bit parameter is sent least significant bit first, so that for a parameter PAR the serial bit holding PAR_B0 precedes the one holding PAR_B1, and so on. The extraction of a field of w bits that starts at serial bit k is therefore the number whose bit j (j = 0 to w - 1) is serial bit k + j.

### 1.4 Field layout of an active frame

The following table lists, for an active frame, the start bit, the width and the content of each field. "Both" means the same for both rates.

| Start | Width | Field | Rate |
|---|---|---|---|
| 0 | 2 | Frame type code (RATEFLAG is bit 0, VADFLAG is bit 1) | Both |
| 2 | 24 | LSP indices, number LspId = 65536 x (index of sub-vector 0) + 256 x (index of sub-vector 1) + (index of sub-vector 2). LPC_B0 is the lowest bit. | Both |
| 26 | 7 | ACL0: lag A minus 18 | Both |
| 33 | 2 | ACL1: lag offset code of subframe 1 | Both |
| 35 | 7 | ACL2: lag B minus 18 | Both |
| 42 | 2 | ACL3: lag offset code of subframe 3 | Both |
| 44 | 12 | GAIN0 | Both |
| 56 | 12 | GAIN1 | Both |
| 68 | 12 | GAIN2 | Both |
| 80 | 12 | GAIN3 | Both |
| 92 | 1 | GRID0 | Both |
| 93 | 1 | GRID1 | Both |
| 94 | 1 | GRID2 | Both |
| 95 | 1 | GRID3 | Both |
| 96 | 1 | Unused bit UB (written as 0, ignored on reading) | High |
| 97 | 13 | MSBPOS, the combined index of the high-order parts of the four position indices | High |
| 110 | 16 | POS0 low part | High |
| 126 | 14 | POS1 low part | High |
| 140 | 16 | POS2 low part | High |
| 156 | 14 | POS3 low part | High |
| 170 | 6 | PSIG0 | High |
| 176 | 5 | PSIG1 | High |
| 181 | 6 | PSIG2 | High |
| 187 | 5 | PSIG3, ending at bit 191 | High |
| 96 | 12 | POS0 | Low |
| 108 | 12 | POS1 | Low |
| 120 | 12 | POS2 | Low |
| 132 | 12 | POS3 | Low |
| 144 | 4 | PSIG0 | Low |
| 148 | 4 | PSIG1 | Low |
| 152 | 4 | PSIG2 | Low |
| 156 | 4 | PSIG3, ending at bit 159 | Low |

The high rate frame has 192 bits (24 octets) and the low rate frame 160 bits (20 octets). These positions reproduce Tables 5 and 6 octet by octet.

Interpretation of the fields:

1. Lag A = ACL0 + 18 and lag B = ACL2 + 18. A decoded lag is between 18 and 141 when the field is valid.
2. The lag offset code of an odd subframe (ACL1 for subframe 1, ACL3 for subframe 3) is a number 0 to 3. The lag actually used by an odd subframe is the lag of its pair plus the code minus 1, so the offset is -1, 0, +1 or +2 as in equation 38. For an even subframe the code is taken to be 1, so the offset is 0. Thus the pitch lag used for the adaptive codebook of subframe s is lag A or lag B plus (code minus 1), where the code is 1 for s = 0 and s = 2.
3. Each GAIN field of subframe s is a 12-bit number g.
   a. At the low rate, and at the high rate when the lag of the pair (lag A for s = 0, 1 and lag B for s = 2, 3) is 58 or more, the pitch gain codebook index is AcGn = g divided by 24 (integer division), the pulse gain index is Mamp = g modulo 24, and the pulse train flag Tran is 0.
   b. At the high rate, when the lag of the pair is less than 58, bit 11 of g is the pulse train flag Tran, and the remaining 11 bits (g modulo 2048) form the number from which AcGn = that number divided by 24 and Mamp = that number modulo 24 are taken. AcGn then indexes the 85-entry codebook.
   c. The value AcGn indexes PITCH_GAIN_CODEBOOK_170 in case a and PITCH_GAIN_CODEBOOK_85 in case b, as in the text of 2.18 (equations 39 and 40). Mamp indexes FIXED_GAIN_LEVELS. In the low rate case AcGn also indexes ACELP_PITCH_TAP_OFFSETS and ACELP_PITCH_TAP_GAINS.
4. GRID is the grid bit of the subframe (0 or 1).
5. High rate position fields. MSBPOS is a 13-bit number. Let q1 = MSBPOS divided by 90 and q2 = MSBPOS modulo 90 (integer division). Then the high-order part of the position index of subframe 0 is q1 divided by 9, of subframe 1 is q1 modulo 9, of subframe 2 is q2 divided by 9, and of subframe 3 is q2 modulo 9. The full position index of a subframe is 65536 times its high-order part plus its POS field for subframes 0 and 2, and 16384 times its high-order part plus its POS field for subframes 1 and 3. The packer builds MSBPOS as ((h0 x 9 + h1) x 90) + (h2 x 9 + h3), where h0 and h2 are the position indices of subframes 0 and 2 divided by 65536 and h1 and h3 are the position indices of subframes 1 and 3 divided by 16384 (integer division). The pulse sign and amplitude field of subframe s (PSIG) is passed to the fixed codebook as the number whose bit j is serial bit (start + j).
6. Low rate position fields. The position index of subframe s is the 12-bit field POS, and the sign field is the 4-bit PSIG, both passed to the fixed codebook decoder.

### 1.5 SID and untransmitted frames

A SID frame has 32 bits: the 2-bit frame type code 2, the 24-bit LspId at start bit 2, and a 6-bit SID gain index at start bit 26 (octet 4 carries it in its bits 7 to 2, with LPC_B23 and LPC_B22 in bits 1 and 0, as in Table A.1). The index is passed to the comfort noise decoder as the quantized SID gain index of subframe 0. An untransmitted frame has only the 2-bit code 3.

### 1.6 Packing (encoder side, for completeness)

The packer clears all 24 output octets, then writes the bit string of the frame in the order of the tables above. The frame type code written is: 0 for an active high rate frame, 1 for an active low rate frame, 2 for a SID frame, 3 for an untransmitted frame. For an active frame the GAIN field written is AcGn x 24 + Mamp, and for the high rate also plus 2048 x Tran (Tran is 0 when the lag of the pair is 58 or more). The reserved bit is written as 0. A frame is written with 24 octets (high rate), 20 octets (low rate), 4 octets (SID) or 1 octet (untransmitted); all unused bits are 0. The octet count written is derived from the two low bits of the first octet by the table of 1.1.

## 2. Unpacking and validity checks (clause 4, reference unpacker)

The unpacker is given the frame octets and the external erasure flag (section 13.4; zero when there is none). It produces a frame type, the decoded parameters and an internal erasure flag. The steps are in this order.

1. If the external erasure flag is nonzero, unpacking stops at once. No bit of the frame is examined, no parameter is produced, and the frame is marked erased. The frame type is not determined here. The speech receiving rate is also left unchanged (see below).
2. Otherwise read the frame type code (2 bits).
3. If the code is 3, the frame type is "untransmitted" (type 0 in the numbering of section 5). Unpacking stops. The frame is not erased.
4. Read the 24-bit LspId.
5. If the code is 2, read the 6-bit SID gain index. The frame type is "SID" (type 2). Unpacking stops. The frame is not erased.
6. Otherwise the frame is active (type 1). The current rate is set to high rate if the code is 0 and to low rate if the code is 1. This assignment happens at this point, before any of the validity checks below, and it is the only assignment of the rate in the decoder. The rate is not changed by erased frames (step 1) and not by SID or untransmitted frames.
7. Read ACL0. If the value is greater than 123 (the values 124 to 127 are forbidden), the frame is declared erased and unpacking stops. Otherwise lag A = ACL0 + 18.
8. Read ACL1.
9. Read ACL2. If greater than 123, the frame is declared erased and unpacking stops. Otherwise lag B = ACL2 + 18.
10. Read ACL3.
11. For subframes 0 to 3 in order, read GAIN and split it as in 1.4 item 3. The pair lag used for the test is lag A for subframes 0 and 1 and lag B for subframes 2 and 3. The bound is 85 if the rate is high and the pair lag is less than 58, and 170 otherwise. If AcGn is not strictly less than the bound, the frame is declared erased and unpacking stops. (In the 85-entry case the 11-bit number is at most 2047, so the test fails exactly when it is 2040 or more. In the 170-entry case the test fails exactly when g is 4080 or more.) Mamp is at most 23 and needs no test.
12. Read the four grid bits.
13. For the high rate: skip the unused bit, read MSBPOS, split it and form the four position indices, then read the four PSIG fields (1.4 item 5). For the low rate read four POS fields and four PSIG fields. No range test is made on position indices or on MSBPOS here; the fixed codebook decoder handles an out-of-range high rate position index by producing a zero vector (section 7.3).

These are all the validity checks. The unused bit is not tested. Position indices are not tested. Pulse sign fields are not tested. The SID and LSP fields are not tested.

On failure (steps 7, 9 and 11) the reference marks the frame as erased in exactly the same way as an erasure indication from the erasure file: the frame is decoded as an erased frame in section 5, step 3. The parameters decoded before the failure are discarded. The only side effect of a failing frame beyond that is the rate assignment of step 6 described above, which is overwritten by the next active frame and is not read by any erased-frame processing, so it has no effect on the output.

The conformance bitstream TEST16.CMH (mixed rates, decoded together with TEST16.BFI) contains three active frames with invalid fields: frame 23 (lag A forbidden, high rate), frame 34 (gain index of subframe 2 out of range, low rate) and frame 146 (lag B forbidden, high rate), counting frames from 0. Its erasure file has a zero word for each of these frames. Those three frames therefore exercise the internal erasure path described here. No other conformance bitstream contains an invalid field.

## 3. Decoder state and initial values (3.11)

The text says that all static variables are zero except the previous LSP vector (the LSP DC vector) and the postfilter gain (1). The complete list of the reference is below; it adds the comfort noise state, whose initial values come from Annex A. "Persists" means that the value carries from frame to frame, including through erased frames, SID frames and untransmitted frames, unless a step says otherwise. All state is independent of the rate of the current frame; there is no reset on a rate change.

| State | Size | Initial value | Updated by |
|---|---|---|---|
| Previous LSP vector | 10 | LSP_DC_VECTOR | Every frame (section 5 step 5 and the comfort noise branch) |
| Excitation memory | 145 | 0 | Every frame (active: last 145 samples of the frame buffer; comfort noise: Annex A) |
| Synthesis filter memory | 10 | 0 | Every frame, last 10 outputs of the synthesis filter |
| Formant postfilter FIR memory | 10 | 0 | Every frame, last 10 inputs of the postfilter |
| Formant postfilter IIR memory | 10 | 0 | Every frame, last 10 outputs of the ARMA part |
| Smoothed first reflection coefficient k1 | 1 | 0 | Every subframe |
| Postfilter scaling gain g | 1 | 1.0 | Every sample |
| Erasure counter | 1 | 0 | Active frames only |
| Interpolation gain (concealment gain) | 1 | 0.0 | Active frames only |
| Interpolation index (concealment lag, 0 means unvoiced) | 1 | 0 | Good active frames only |
| Concealment random seed | 1 | 0 | Unvoiced concealment only (section 10.4) |
| Previous frame type (PastFtyp) | 1 | 1 | Every frame, last step of section 5 |
| Comfort noise state (decoded SID gain / energy estimate, current gain, saved SID LSP vector, comfort noise random seed) | | SID gain 0.0, saved LSP vector LSP_DC_VECTOR, seed 12345; the current gain is not initialised but is written before it is ever read | Annex A document |
| Current rate | 1 | high rate | Unpacker, section 2 step 6 |

The initialisation of the reference (Init_Decod of 3.11 together with the initialisation of the comfort noise state) sets exactly the initial values listed. It is executed at program start and again after every N frames when the option -R is used (section 13.3).

## 4. Overview of the decoding of one frame (3.1, Figure 2)

The signal path of an active frame is as in Figure 2: the decoded excitation is passed through the pitch postfilter, the result through the LPC synthesis filter with the interpolated quantized LPC coefficients, then through the formant postfilter, and the gain scaling unit adjusts the energy of the formant postfilter output to that of the synthesis filter output. The reference follows Figure 2 exactly. The pitch postfilter works on the excitation, before the synthesis filter. The synthesis filter works on the pitch postfiltered excitation and the gain scaling unit receives the unfiltered synthesis output as reference energy.

The postfilter is on by default. The reference has a command line switch (-Nop) that disables it; none of the conformance tests use it. All statements below assume it is on. If it is off, in good active frames the unclipped excitation (section 7.5) goes directly to the synthesis filter, the formant postfilter and gain scaling are skipped, and the postfilter memories and the scaling gain are not touched.

## 5. Frame decoding procedure

Inputs of a frame: the frame octets and the external erasure flag. Output: 240 samples in double precision, converted to 16 bits by section 12.

1. Unpack (section 2). This yields the frame type (0 untransmitted, 1 active, 2 SID), the parameters and the internal erasure flag. "Erased" below means the external flag was nonzero or the internal flag was set by a validity failure.
2. If the frame is erased, the frame type is reassigned: it becomes 1 (active) if the previous frame type is 1, and 0 (untransmitted) otherwise (A.5.2, equation A-21). The previous frame type is the value stored by step 9 of the previous frame (initially 1).
3. If the frame type is not 1, the comfort noise branch (section 11) produces the 240-sample excitation and the four sets of 10 LPC coefficients (one per subframe). Go to step 7. Steps 4 to 6 are skipped in this case, in particular the erasure counter, the concealment gain, the concealment index and the concealment seed are not touched.
4. Active frame. Update the erasure counter: if the frame is erased, add 1, otherwise set it to 0. Then limit it to a maximum of 3. (It is only reset by a good active frame; SID and untransmitted frames, which do not execute this step, leave it unchanged.)
5. Decode the LSP vector of the frame (section 6, using the erased-frame variant when the frame is erased), interpolate it with the previous frame's vector to obtain four LPC sets, and store the decoded vector as the previous LSP vector.
6. Excitation (sections 7 and 10):
   a. If the erasure counter is 0: the frame is a good active frame. Set the concealment gain to the entry of FIXED_GAIN_LEVELS whose index is (Mamp of subframe 2 + Mamp of subframe 3) divided by 2, integer division (rounding down). Decode the excitation of the four subframes (section 7), classify it for later concealment (section 10.1), compute the pitch postfilter (section 8), clip (section 7.5), apply the pitch postfilter and save the decoded LSP vector as the saved SID LSP vector of the comfort noise state.
   b. If the erasure counter is not 0: multiply the concealment gain by 0.75, then regenerate the excitation by the concealment procedure (section 10.2). Neither the concealment index nor the saved SID LSP vector is changed.
   c. In both cases, then: set the excitation memory to the last 145 samples of the 385-sample frame buffer (sections 7.1 and 10.2) and set the comfort noise random seed to 12345.
7. Synthesis. For each subframe s = 0 to 3, in order: filter the 60 samples of the excitation with the synthesis filter of that subframe (section 9.1), and then, if the postfilter is on, apply the formant postfilter (section 9.2) and the gain scaling (section 9.3). This step runs for every frame type, including comfort noise and concealed frames, and all filter memories (synthesis, formant postfilter, k1 and scaling gain) run on continuously.
8. Convert and output the 240 samples (section 12).
9. Store the frame type as the previous frame type. (The order relative to step 7 is irrelevant because synthesis does not read it.)

Note the order inside step 6a: the pitch postfilter parameters and the voicing classification are computed from the unclipped excitation of the new frame, and the pitch postfilter is applied to the clipped excitation. This is explained in 7.5 and 8.4.

## 6. LSP decoding and interpolation (3.2, 3.3, 3.10.1)

As in the encoder analysis document, LSP inverse quantization and LSP interpolation, with the following decoder content.

### 6.1 Inverse quantization

1. Split LspId into three 8-bit indices. Sub-vector 0 (coefficients 0 to 2) uses bits 23 to 16, sub-vector 1 (coefficients 3 to 5) uses bits 15 to 8, sub-vector 2 (coefficients 6 to 9) uses bits 7 to 0. The three tables are LSP_BAND0_CODEBOOK, LSP_BAND1_CODEBOOK, LSP_BAND2_CODEBOOK and the layout is LSP_SUBVECTOR_LAYOUT. The concatenation of the three selected rows is the residual vector r(0) to r(9).
2. A good frame uses the predictor coefficient beta = 12/32 = 0.375 and the minimum distance D = 2.0 (31.25 Hz). An erased frame (external flag or internal validity failure) uses all three indices equal to 0, which selects the all-zero row of each codebook (so r is the zero vector, which is the "vector e set to zero" of 3.10.1), the predictor coefficient beta = 23/32 = 0.71875, and the minimum distance D = 4.0 (62.5 Hz). The LspId of an erased frame is not used.
3. For j = 0 to 9, in this order of operations: p(j) = r(j) + (prev(j) - dc(j)) x beta + dc(j), where prev is the previous LSP vector and dc is LSP_DC_VECTOR.
4. Stability procedure, at most 10 passes. Each pass does the following in order.
   a. If p(0) is less than 3.0, set it to 3.0.
   b. If p(9) is greater than 252.0, set it to 252.0.
   c. For j = 1 to 9 in increasing order: let t = D + p(j-1) - p(j). If t is strictly greater than 0, set t to t x 0.5, then subtract t from p(j-1) and add t to p(j). (The vector is updated in place, so the change to p(j) is seen by the next j.)
   d. The vector is stable if for every j = 1 to 9 the difference p(j) - p(j-1) is not strictly less than D - 0.03125. If stable, stop the passes.
5. If the vector is not stable after the 10th pass, replace p by the previous LSP vector. Otherwise p is the decoded vector.

### 6.2 Interpolation and conversion

The decoded vector p becomes the LSP vector of subframe 3. For subframe s = 0 to 3, the LSP vector is (1 - w(s)) x prev + w(s) x p, with w taken from LSP_INTERPOLATION_WEIGHTS (0.25, 0.5, 0.75, 1.0). Each is converted to a set of 10 LPC coefficients by the LSP to LPC conversion of the encoder analysis document (cosine table LSP_COSINE_TABLE with linear interpolation, polynomial scales LSP_POLYNOMIAL_SCALES). The 10 coefficients produced for a subframe are the coefficients a(1) to a(10) of equation 48, used directly with no further scaling. After the four conversions, the previous LSP vector is replaced by p. This replacement happens in an erased frame too, with the vector decoded by the erased-frame procedure.

## 7. Excitation decoding (2.17, 2.18, 3.4, 3.5)

### 7.1 Buffer

The decoder builds a buffer b of 385 values. Entries 0 to 144 are the excitation memory (b(145 + n) is e(n); b(145 - m) is e(-m)). Entries 145 to 384 receive the 240 samples of the new frame. After decoding (section 5, step 6c), the new excitation memory is entries 240 to 384 of the buffer, that is, e(95) to e(239) of the frame.

### 7.2 Subframe loop (good active frame, erasure counter 0)

For subframe s = 0 to 3, with start n0 = 60 x s:

1. Fixed codebook vector. The fixed codebook decoder (fixed codebook document) is called with the subframe parameters of section 1.4 and the current rate, and with the pair lag (lag A for s = 0, 1 and lag B for s = 2, 3). It returns a 60-sample vector v(n). It is written into the buffer positions of the subframe, e(n0 + n) = v(n).
2. Adaptive codebook vector. The adaptive codebook vector u(n) of the subframe is computed (7.4) from the buffer contents before position n0, which include the already decoded subframes of the current frame (unclipped, see 7.5).
3. The excitation of the subframe is e(n0 + n) = v(n) + u(n), n = 0 to 59. The sum is stored in the buffer immediately, before the next subframe is decoded.

### 7.3 Fixed codebook interface and frame-level effects of invalid parameters

The fixed codebook decoder takes: the rate, the subframe number, Mamp, the grid bit, the position index, the sign field PSIG, Tran, AcLg (offset code) and AcGn, and the pair lag. At the high rate the amplitudes are plus or minus FIXED_GAIN_LEVELS[Mamp]. If the position index is greater than or equal to the entry of MPMLQ_CODEBOOK_SIZES for the subframe (593775 for subframes 0 and 2, 142506 for subframes 1 and 3), the output vector is all zero and the pulse train step is not performed. This is the only handling of out-of-range high rate positions; the frame is not marked erased and the adaptive codebook part of the subframe is still decoded and added. At the high rate, when the pair lag is less than 58 and Tran is 1, the pulse train (repetition with the pair lag) of 2.17 item 5 is applied. At the low rate the pitch synchronous filter of 2.16, controlled by ACELP_PITCH_TAP_OFFSETS and ACELP_PITCH_TAP_GAINS indexed by AcGn, with the lag (pair lag + AcLg - 1 + offset), is applied inside the fixed codebook decoder; it is part of the vector v and no further step is needed at frame level.

### 7.4 Adaptive codebook contribution (2.18, equations 41.1 and 41.2)

For subframe s with pair lag P and lag offset code c (c = 1 for s = 0, 2; the transmitted ACL1 or ACL3 for s = 1, 3), let the lag be M = P + c - 1. The signal e' is built from the buffer as follows, with n0 the subframe start:

1. e'(0) = e(n0 - M - 2) and e'(1) = e(n0 - M - 1).
2. For i = 0 to 61: e'(2 + i) = e(n0 - M + (i modulo M)), where (i modulo M) is the remainder of i divided by M.
3. The contribution is u(n) = Σ (j = 0 to 4) of beta(j) x e'(n + j), n = 0 to 59, where beta(0) to beta(4) are the first five entries of row AcGn of the pitch gain codebook.

The codebook is PITCH_GAIN_CODEBOOK_85 if the rate is high and P is less than 58, and PITCH_GAIN_CODEBOOK_170 in every other case (low rate, or high rate with P of 58 or more). The choice uses the pair lag P (not M). Only columns 0 to 4 of a row (the five tap gains) are used here.

The text says e'(n) = e((n modulo L) - L) for 2 <= n <= 63; the reference repeats the last M samples with the phase starting at n = 2, i.e. uses (n - 2) modulo M as written above. The reference is authoritative. For M of 62 or more the repetition does not occur, because i is at most 61. The smallest possible M is 17 (P = 18, c = 0), and all indices e(n0 - M - 2) down to e(-143 - 2) lie inside the 145-sample memory of the frame buffer, because the largest M is 143 (P = 141 and c = 3) and n0 - M - 2 is at least -145.

### 7.5 Clipping and storing the excitation

After all four subframes are decoded:

1. The 240 new excitation samples are copied to the output frame buffer (the "frame excitation") without change. This copy is unclipped.
2. The voicing classification (10.1) and the pitch postfilter parameters (section 8.1 to 8.3) are computed from the buffer holding the unclipped excitation.
3. Then the 240 new samples of the buffer are clipped: a value strictly less than -32767.5 becomes -32768.0, a value strictly greater than 32766.5 becomes 32767.0, and any other value is left unchanged. There is no rounding to integer. Both thresholds are the same as those of the output conversion in section 12.
4. The pitch postfilter filtering (8.4) reads the clipped buffer. The excitation memory for the next frame is taken from the clipped buffer.

Therefore the excitation used inside a frame by later subframes (adaptive codebook), by the classifier, and by the pitch postfilter parameter search is unclipped, whereas the postfiltered excitation, and the excitation memory carried to the next frame, are clipped. The sample range of 16 bits can only be exceeded with extreme parameters, but the test suite includes such streams ("overflow" vectors OVERC53H and OVERC63).

If the postfilter is off, the frame excitation that goes to the synthesis filter is the unclipped one, while the memory is still taken from the clipped buffer.

## 8. Pitch postfilter (3.6)

The postfilter acts on the excitation of each subframe s = 0 to 3 separately, with start n0 = 60 s, and it uses the pair lag P (lag A for s = 0, 1, lag B for s = 2, 3). If P is greater than 142 it is replaced by 142; that cannot occur for valid streams (maximum lag 141). The rate used is the rate of the current frame. Each subframe has three results: a delay M (an integer, negative for backward, positive for forward, 0 for none), a gain G, and a scaling gain S. The initial values are M = 0, G = 0 and S = 1.

The search uses e(n), the unclipped buffer, including the samples of the later subframes of the same frame (needed for forward delays) and the 145-sample memory.

### 8.1 Backward search

For each Mb from P - 3 to P + 3 in increasing order, compute Cb(Mb) = Σ (n = 0 to 59) e(n0 + n) x e(n0 + n - Mb). Keep the largest value, with strict comparison against the best so far (so on ties the smaller Mb wins), starting from a best value of 0.0. If no Cb is strictly greater than 0, there is no backward candidate (Mb = 0). The reference returns the backward delay as -Mb (negative).

### 8.2 Forward search

The same, but for each Mf from P - 3 to P + 3 in increasing order, the candidate is only evaluated if n0 + 60 + Mf is not strictly greater than 240 (the last sample used, e(n0 + 59 + Mf), must be within the current frame, 239 or less). Cf(Mf) = Σ (n = 0 to 59) e(n0 + n) x e(n0 + n + Mf). Selection as in 8.1 (strictly greater than the best so far, initial best 0.0). The test n0 + 60 + Mf not greater than 240 is the same as Mf not greater than 180 - 60 s. Since every candidate delay is at least 15, subframe 3 never has a forward candidate. Subframe 2 allows delays up to 60, subframe 1 up to 120, and subframe 0 up to 180, which admits every candidate.

The text says that the weight and delay are set to 0 if "for some n there is no sample value e[n + Mf] available"; the reference test is exactly the condition above. The text does not say what happens for the samples beyond the current frame: the reference never uses samples beyond the frame, and never uses samples of the next frame.

### 8.3 Selection and gains (equations 43 to 47)

Let Ten = Σ (n = 0 to 59) of e(n0 + n) squared.

1. If both candidates are absent: M = 0, G = 0, S = 1. The subframe passes unchanged.
2. If only the backward candidate exists, the selected delay is Mb with C = Cb(Mb) and D = Db = Σ (n = 0 to 59) of e(n0 + n - Mb) squared. If only the forward candidate exists, the selected delay is Mf with C = Cf(Mf) and D = Df = Σ of e(n0 + n + Mf) squared.
3. If both exist: compute Db and Df as above. The backward candidate is selected if Df x Cb squared is strictly greater than Db x Cf squared, otherwise the forward candidate is selected (ties go to forward). This is the comparison of Cb squared / Db with Cf squared / Df of the text, written without division.
4. For the selected candidate (M, C, D):
   a. Prediction gain test. The candidate is accepted only if C squared is strictly greater than Ten x D / 4. (This is the 1.25 dB test of the text: the prediction gain -10 log10(1 - C squared / (D x Ten)) exceeds 1.25 dB exactly when C squared / (D x Ten) exceeds 0.25 approximately; the reference uses exactly one quarter, which corresponds to 1.249 dB, not 1.25 dB. A comment in the reference calls it a 2 dB test; the formula is the authority.)
   b. If the candidate is not accepted, the subframe is not filtered: G = 0 and S = 1, with the delay M recorded but of no effect.
   c. If accepted: let gamma_ltp = PITCH_POSTFILTER_GAIN_WEIGHTS[rate], that is 0.1875 for the high rate and 0.25 for the low rate. The gain g is gamma_ltp if C is greater than or equal to D, and gamma_ltp x (C / D) otherwise. (Equation 46 with the weight.) Then compute E = Ten + 2 x C x g + g x g x D, which is the energy of e + g x (delayed e). If the absolute value of E is strictly less than 1.17549435e-38, set S = 0, otherwise S = sqrt(Ten / E). Finally G = g x S.
5. The text says that if the denominator of equation 47 is less than the numerator the gain is set to 1. In the reference this check does not exist; it never triggers because C is strictly positive in an accepted candidate, so E is greater than Ten.

The gain weights wf and wb of the text are expressed in the reference by the selection: exactly one of them is 1 and the other 0 if the subframe is filtered.

### 8.4 Filtering

For each subframe s and n = 0 to 59: ppf(n0 + n) = S x e(n0 + n) + G x e(n0 + n + M), where e is the clipped buffer (7.5) and M is the recorded delay of the subframe (M is negative for backward). This is equation 42 with gp = S and the weights folded into G. If M = 0, G = 0 and S = 1 the output is equal to the clipped input. The output replaces the frame excitation for the synthesis filter. Because the filtering reads the buffer and writes to the separate frame excitation array, the output of one subframe does not influence the input of another. The memory for the next frame is not postfiltered (7.5, item 4).

The postfilter is applied only to good active frames. Erased active frames (section 10) and comfort noise frames are not pitch postfiltered, but they do go through the synthesis filter, the formant postfilter and the gain scaling.

## 9. Synthesis, formant postfilter and gain scaling (3.7, 3.8, 3.9)

All three are applied subframe by subframe, in order, to each 60-sample block, with the LPC set a(1) to a(10) of that subframe (section 6.2). All memories are those of section 3 and carry over to the next subframe and the next frame.

### 9.1 LPC synthesis filter (3.7, equation 48)

For n = 0 to 59: sy(n) = x(n) + Σ (j = 1 to 10) of a(j) x sy(n - j), where x is the (pitch postfiltered) excitation of the block and sy(n - j) for n - j negative are the stored last 10 outputs of the previous block (previous subframe, or previous frame). The memory is the last 10 outputs of this filter. The output sy of the block replaces x.

### 9.2 Formant postfilter (3.8, equations 49.1 to 49.3)

Let sy(0) to sy(59) be the block from 9.1. The postfilter changes them to pf(n).

1. Energy and correlation. R0 = Σ (n = 0 to 59) of sy(n) squared. R1 = Σ (n = 0 to 58) of sy(n) x sy(n + 1). (This is the same sum as the sum from n = 1 to 59 of sy(n) x sy(n - 1) in the text.) The block energy R0 is also kept as the reference energy for the gain scaling.
2. k = R1 / R0 if R0 is strictly greater than 1.17549435e-38, otherwise k = 0.
3. k1 = 0.75 x k1old + 0.25 x k, where k1old is the stored value from the previous subframe (initially 0). Store k1 as the new k1old. The tilt coefficient is t = -0.25 x k1, which is the coefficient of z to the power -1 in the last factor of equation 49.3.
4. Coefficient arrays: Fir(j) = a(j) x POSTFILTER_ZERO_FACTORS[j - 1] (lambda1 to the power j = 0.65 to the power j) and Iir(j) = a(j) x POSTFILTER_POLE_FACTORS[j - 1] (lambda2 to the power j = 0.75 to the power j), j = 1 to 10.
5. For n = 0 to 59, in order:
   a. y(n) = sy(n) - Σ (j = 1 to 10) of Fir(j) x sy(n - j) + Σ (j = 1 to 10) of Iir(j) x y(n - j). Here sy(n - j) for negative argument is the stored FIR memory (the unfiltered input samples of earlier blocks, that is, the synthesis output) and y(n - j) is the stored IIR memory (the outputs y of earlier samples, before the tilt stage). Both memories are updated by sliding in sy(n) and y(n).
   b. pf(n) = y(n) + t x y(n - 1), where y(-1) is the last stored IIR memory value at the start of the block.
6. The block pf replaces sy. The result must be formed as in 5 for all 60 samples (the filter runs sample by sample and the memories are updated each sample).

### 9.3 Gain scaling (3.9, equations 50 to 52)

1. Let E1 = Σ (n = 0 to 59) of pf(n) squared, the energy of the formant postfilter output, and E0 = R0 the energy of the block before the formant postfilter (9.2 step 1).
2. If E1 is strictly greater than 1.17549435e-38, let c = sqrt(E0 / E1) x 0.0625, otherwise let c = 0.0625. This is alpha x gs of equation 51 with alpha = 1/16, where gs = sqrt(E0 / E1), and gs = 1 in the second case.
3. For n = 0 to 59, in order: g = 0.9375 x g + c (this is equation 51, g(n) = (1 - alpha) g(n - 1) + alpha gs), then q(n) = ((1.0625 x pf(n)) x g), evaluated in that order of multiplication (equation 52, (1 + alpha) = 1.0625). The gain g carries from sample to sample, across subframes and across frames, with initial value 1.0.
4. The block q is the decoder output signal for the subframe.

The text says "If the denominator is equal to 0, gs is set to 1": the reference tests against 1.17549435e-38 instead of 0.

## 10. Frame erasure concealment (3.10)

A frame is concealed only if its frame type, after the reassignment of section 5 step 2, is active. Erased frames following a SID or untransmitted frame are handled by the comfort noise branch. The LSP part of an erased active frame was already described in 6.1 (predictor 23/32, minimum distance 62.5 Hz). The residual part is below.

### 10.1 Voiced/unvoiced classifier (Comp_Info)

Run once for each good active frame (erasure counter 0), after all four subframes are decoded and before clipping, on the unclipped buffer b, with the lag B (the lag of the third subframe, L2). If lag B is greater than 142 it is replaced by 142 (cannot happen for valid streams).

1. The last 120 samples of the frame are e(120) to e(239). For each lag m from lag B - 3 to lag B + 3 in increasing order, compute Σ (n = 0 to 119) of e(120 + n) x e(120 + n - m). Keep the best value with the lag, using strict comparison to the best so far, starting from a best value of 0.0 and a best lag of lag B (so if no correlation is strictly positive the best lag stays lag B and the best value is 0). Call the best value Ccr and the best lag Ibest.
2. Tenr = Σ (n = 0 to 119) of e(120 + n) squared. Enr = Σ (n = 0 to 119) of e(120 + n - Ibest) squared.
3. Tenr is stored as the "energy estimate" of the comfort noise state (the stored SID gain of the Annex A document, which is overwritten by every good active frame). The text of A.5.1 calls this energy Enr. This value is read by the comfort noise branch only when the first SID frame of an inactive period is lost.
4. The classification result, the interpolation index, is: 0 (unvoiced) if Ccr is less than or equal to 0; otherwise Ibest if 0.125 x Enr x Tenr - Ccr x Ccr is strictly less than 0, that is if Ccr squared is strictly greater than Enr x Tenr / 8; otherwise 0. The 0.58 dB of the text corresponds to a factor of 1/8 (0.5799 dB). The result is stored as the interpolation index.

The text says that "the average of the gain indices for subframes 2 and 3 is saved" if the frame is unvoiced. The reference saves the gain, FIXED_GAIN_LEVELS[(Mamp(2) + Mamp(3)) divided by 2] as the concealment gain (section 5 step 6a), in every good active frame, voiced or not; it is used only when the later concealment is unvoiced.

### 10.2 Concealment of an erased active frame (Regen)

The erasure counter is 1, 2 or 3 (it is capped at 3). The concealment gain has already been multiplied by 0.75 (section 5 step 6b), so after the first erased frame it is 0.75 times the saved gain, after the second 0.5625 times, and so on. The interpolation index is the one stored by the last good active frame (it is not recomputed during erasures). The buffer b has the previous excitation memory in entries 0 to 144 (as left by the previous frame, including its own concealment). The frame excitation is generated as follows.

1. If the erasure counter is 3 (third and later consecutive erased active frames, with no good active frame in between): the frame excitation is all zero, and the whole buffer (385 values) is set to zero, so that the excitation memory is also zero afterwards. This is the muting after 3 interpolated frames.
2. Otherwise, if the interpolation index L is not 0 (voiced): the periodic regeneration. For n = 0 to 239 in increasing order, set e(n) = e(n - L), reading values that were just written once n is at least L (the first L samples copy the last L samples of the memory, and the pattern then repeats with period L). Then, in a second pass over all 240 samples, multiply each by 0.75, store it back into the buffer and into the frame excitation. The scaling is applied after the whole periodic sequence is built, so the periodic extension is built from unattenuated samples. The excitation memory of the next frame is the last 145 samples of this scaled sequence, so a second consecutive erased frame regenerates from a signal that was already attenuated by 0.75, giving a total attenuation of 0.75 x 0.75 = 0.5625 (about -5 dB).
3. Otherwise (unvoiced, index 0): the frame excitation is e(n) = concealment gain x r(n) / 32768 for n = 0 to 239, where r(n) is the n-th number produced by the random generator of 10.4 in order (the generator is advanced once per sample, 240 times per frame). The whole buffer (385 values) is then set to zero, and therefore the excitation memory for the next frame is zero, not the random excitation (the random excitation is only used for this frame's synthesis).

The attenuation per erased frame (a factor of 0.75 = -2.5 dB) agrees with the text ("an additional 2.5 dB for each frame"), but the exact schedule is: erased frame 1 has the factor 0.75, erased frame 2 has the factor 0.5625 (voiced) or the gain 0.5625 times the saved gain (unvoiced), and erased frame 3 and later are muted. The reference does not clip the concealed excitation.

### 10.3 Resetting the erasure counter

The counter returns to 0 only in a good active frame. Frames of types SID and untransmitted neither reset nor increment it. After a good active frame, the interpolation index and concealment gain are recomputed from that frame. A good active frame does not reset the random seed of 10.4.

### 10.4 Random generator

The concealment generator has a 16-bit state s, initially 0 (set at initialisation of the decoder only, and not reset by good frames). Each draw computes s = (521 x s + 259) modulo 65536 (arithmetic modulo 2 to the power 16, so that the sequence of 16-bit patterns is the same whether s is regarded as signed or unsigned), and returns the new s interpreted as a signed 16-bit number in two's complement, in the range -32768 to 32767. The first two values from the initial state are 259 and 4126. The concealment scales this signed value by 1/32768. The same generator (with the unsigned value masked to 15 bits) is used by the comfort noise generator with its own seed (Annex A document).

## 11. Comfort noise branch (Annex A.5)

The decoder calls the comfort noise decoder (Dec_Cng of the Annex A document) when the frame type of section 5 step 2 is not 1: either a received SID frame (type 2), a received untransmitted frame (type 0), or an erased frame whose predecessor was not an active frame (reassigned to type 0). The call passes the frame type and, for a SID frame, the 6-bit SID gain index and the LspId. The branch:

1. Reads: the previous frame type (the stored value of the preceding frame; initially 1), the SID gain / energy estimate (set by SID frames, and overwritten by section 10.1 step 3 in good active frames), the current comfort noise gain, the saved SID LSP vector (set by SID frames; set in good active frames by section 5 step 6a), the comfort noise random seed, the previous LSP vector, and the excitation memory.
2. Writes: the SID gain / energy estimate (decoded from the SID index, or, when the previous frame type is 1 and the frame is not a SID frame, restored by quantizing the stored energy estimate), the current comfort noise gain, the saved SID LSP vector (SID frames only; for a SID frame it is decoded with the normal good-frame LSP inverse quantizer of section 6.1, that is predictor 12/32 and minimum distance 2.0, using the previous LSP vector), the comfort noise random seed, the previous LSP vector (set equal to the saved SID LSP vector in every frame of this branch), and the excitation memory (it is replaced by the comfort noise excitation: updated inside the branch, not by section 5 step 6c).
3. Returns the 240-sample excitation and the four interpolated LPC sets (computed from the saved SID LSP vector and the previous LSP vector by the interpolation of section 6.2).

This branch does not read or write: the erasure counter, the concealment gain, the interpolation index, the concealment seed, the postfilter memories, k1 and the scaling gain, or the synthesis filter memory. The synthesis, formant postfilter and gain scaling of section 5 step 7 are then applied exactly as for active frames. The comfort noise excitation is clipped inside the branch.

The comfort noise seed (initial and reset value 12345) is reset to 12345 by every active frame (section 5 step 6c), good or erased. The comfort noise excitation uses the adaptive codebook procedure of section 7.4 with pair lags of at least 123, so the 170-entry codebook is chosen at either rate and the current rate has no effect on it.

## 12. Output conversion

The reference converts each of the 240 output samples x (a double) to a 16-bit signed integer as follows.

1. If x is strictly less than -32767.5, the output is -32768.
2. Otherwise if x is strictly greater than 32766.5, the output is 32767.
3. Otherwise, if x is less than 0, the output is the integer obtained by subtracting 0.5 from x and truncating toward zero; otherwise (x is 0 or positive) it is the integer obtained by adding 0.5 and truncating toward zero.

This is rounding to the nearest integer with halves rounded away from zero (for example 2.5 gives 3, -2.5 gives -3). Samples are written as 16-bit little-endian two's complement numbers, 240 per frame, with no header. There is no scaling or gain at this stage: the excitation, the synthesis filter, and the postfilter work in the 16-bit sample scale, and the double value is the PCM value. At the two limits step 3 gives the same result as steps 1 and 2 would: x = -32767.5 gives -32768 and x = 32766.5 gives 32767.

The conformance comparison is done on these 16-bit values (section 13.6).

## 13. Annex B test harness and test vector formats

### 13.1 Command line of the reference program

The program is invoked as: options, input file, output file. The options relevant to the decoder are:

| Option | Meaning |
|---|---|
| -d | Decode only: the input is a bitstream file and the output is a PCM file. |
| -c | Encode only. |
| -b | Both (default): encode and then decode in memory. In this mode no frame is ever erased. |
| -r63, -r53 | Select the encoder rate. In decode mode it has no effect on decoding, because the rate of every frame is read from the frame itself (section 2 step 6). |
| -rNAME | (Encoder) NAME is a rate file, see 13.5. In decode mode a rate file is not opened. |
| -fNAME | (Decoder) NAME is the frame erasure file, see 13.4. It is read only in decode mode. |
| -v | (Encoder) enable VAD and comfort noise generation. The decoder needs no such switch: SID and untransmitted frames are recognised in the bitstream. |
| -Noh | (Encoder) disable the high-pass filter. |
| -Nop | Disable the postfilters (pitch and formant postfilter and gain scaling). Default is on. |
| -n | Suppress progress messages. |
| -Rn | Re-initialise, see 13.3. |

The options -fNAME and -rNAME have the file name attached to the letter with no space. The postfilter is enabled in the default configuration and in every conformance test of the suite.

### 13.2 Input and output files

All 16-bit PCM files and all erasure files are little-endian. PCM files hold signed 16-bit samples at 8 kHz, mono, with no header; a frame is 240 samples (480 octets). Bitstream files are as in section 1.2. The decoder output for N frames has N x 480 octets. In the encoder, a final partial input frame is padded with zeros; this is not relevant to the decoder.

### 13.3 The -R option

With -Rn (n greater than 0), after every n-th frame has been completely processed (including its output), the program re-initialises all its state: the encoder state, the VAD and encoder comfort noise state (if -v is on), the decoder state, and the decoder comfort noise state. The counter counts frames of the current run from 1, so the reset occurs after frames n, 2n, 3n and so on, so every group of n frames is coded as an independent sequence. It applies to every mode, including decoding, although no decoder test uses it. The effect on the decoder if used: the decoder state returns to the initial values of section 3 including the previous LSP vector, the previous frame type 1, and the erasure counter 0. The encoder tests use -R10 so the encoder processes each 10-frame group independently; the encoded test streams (.c53 and .c63) were produced that way.

### 13.4 Erasure files (.bfi)

A frame erasure file contains one 16-bit little-endian word per frame, in frame order. The decoder reads one word before decoding each frame. A zero word means the frame is good, and any nonzero word means the frame is erased. The test files hold only the values 0 and 1. The word is read after the frame's octets have been read from the bitstream file, so an erased frame still occupies its normal length in the bitstream. If the erasure file ends before the bitstream, the reference keeps the last word read (the last value repeats). The conformance erasure files have exactly one word per frame: DM29.BFI has 163 words and TEST16.BFI has 742 words.

When no erasure file is given every frame is treated as not erased (but internal validity failures of section 2 still apply).

### 13.5 Rate files (mixed-rate encoding)

A rate file is a sequence of single octets, one per input frame of the encoder, read before each frame is encoded. The octet value 0 selects the high rate (6.3 kbit/s) for that frame and the value 1 the low rate (5.3 kbit/s). The octet is used as a number; no other values are defined. If the rate file ends early, the last value remains in effect. Rate files are an encoder input; the decoder derives the rate of each frame from the bit stream. No rate file is part of the test vector directory; the mixed-rate bitstreams TEST16.CMH and VAD6353.CMV carry the rates in their frames.

### 13.6 Comparison programs

checksnr compares two PCM files sample by sample. Both files are read as signed 16-bit numbers, and must have the same number of samples (otherwise the program stops with an error). With x(n) the samples of the first file (the reference) and y(n) those of the second file (the decoder under test), both converted to double precision, Energy = Σ x(n) squared and Error = Σ (x(n) - y(n)) squared, summed over all samples of the file. If Error is 0, the result is a pass (infinite SNR). Otherwise, if Energy is 0 the result is a fail. Otherwise SNR = 10 x log10(Energy / Error), in decibels, over the whole file, and the test passes if SNR is greater than or equal to the threshold.

cmpcode compares two bitstream files (encoder tests). It reads groups of 10 consecutive frames (or the number given by the option -R) from each file, using the frame length rule of section 1.2, and concatenates the octets of a group. A group matches if the two concatenations have the same total length and are identical in all octets, including unused bits. The result is 100 x (number of matching groups) / (number of groups in the first file) percent, where the last group of a file may be shorter than the others. The encoder passes if the result is at least 95 percent. The program fails if the files have a different number of groups.

### 13.7 Decoder tests and thresholds

Each test decodes a bitstream with the options shown and requires the SNR against the reference PCM file to be at least the threshold, in dB. The postfilter is on, no -R is used.

1. Rate 5.3 streams without comfort noise (files with extension C5H, references R5P): DM29 with erasure file DM29.BFI (reference B5P) 63, DM29 without erasures 63, DMBIS 62, IN1 75, IN2 80, IN3 58, IN4 60, IN5 64, IN6 77, INEQC53 59, OVERC53H 84, PATHC53 51, READ7 58, TEST16 61.
2. Rate 6.3 streams without comfort noise (C6H, R6P): CODEC63 59, DM29 with DM29.BFI (B6P) 63, DM29 63, DMBIS 63, IN1 77, IN2 81, IN3 58, IN4 60, IN5 64, IN6 75, OVERC63 77, PATHC63H 61, READ7 58, TEST16 61.
3. Mixed rates without comfort noise: TEST16.CMH with TEST16.BFI, reference TEST16.BMP, 60.
4. Rate 5.3 streams with comfort noise (C5V, R5V): DM29 63, DMBIS 62, IN5 64, PATHC53 51, READ7 58, TEST16 60, VAD6353 57.
5. Rate 6.3 streams with comfort noise (C6V, R6V): DM29 63, DMBIS 62, IN5 64, PATHC63H 61, READ7 58, TEST16 60, VAD6353 57.
6. Mixed rates with comfort noise: VAD6353.CMV with VAD6353.BFI, reference VAD6353.BMV, 57.

Reference outputs for the comfort noise streams (R5V, R6V, BMV) conform to the version 5.2F of the reference (the corrected random lag B generation of the comfort noise excitation and the corrected comparison in the encoder energy test).

## 14. Differences between the text and the reference, and points that need care

1. Adaptive codebook repetition (7.4): text e'(n) = e((n modulo L) - L); the reference uses the phase (n - 2) modulo L.
2. Forward lag availability (8.2): text speaks of missing samples for some n; the reference allows a forward delay only if the subframe start plus 60 plus the delay is not greater than 240.
3. The 1.25 dB test (8.3 step 4a) is a test against the exact fraction 1/4, strict.
4. The text says that the postfilter gain is set to 1 if the denominator of equation 47 is smaller than the numerator; the reference has no such test and it cannot trigger.
5. The text says the gain scaling unit sets gs to 1 if the denominator is 0; the reference compares with 1.17549435e-38. The same constant appears in the formant postfilter k computation and in the pitch postfilter gain scaling.
6. The saved concealment gain is stored for every good active frame, not only for unvoiced ones (10.1).
7. The attenuation of erased frames is 0.75 per frame, applied to the already attenuated memory in the voiced case, and via the concealment gain multiplied by 0.75 per frame in the unvoiced case. The muting happens at the third consecutive erased active frame.
8. The unvoiced concealment excitation is not written to the excitation memory; the memory is zeroed instead (10.2 step 3).
9. The untransmitted frame (1 octet, type code 3) is a reference convention; the text says the code is reserved (3).
10. Frames that fail the lag or gain index validity checks are treated as erased with the same effect as an externally flagged erasure (section 2). The text does not mention these checks.
11. The erasure counter is not touched by SID and untransmitted frames, and the interpolation index and gain are not touched by erased frames; both persist through comfort noise periods.
12. The clipping of section 7.5 is applied to the postfilter input and to the memory, but not to the intra-frame excitation, and not to the frame excitation when the postfilter is off. The decision whether an implementation reproduces the reference at the clipping limit depends on this ordering.
13. The text of 3.6 gives gamma ltp as 0.1875 for the high rate and 0.25 for the low rate; this is PITCH_POSTFILTER_GAIN_WEIGHTS.
14. If a frame is externally flagged erased but contains a SID frame in the bitstream, the reference does not look at the content; the frame is reassigned by the rule of section 5 step 2. A SID frame at an erased position after an active frame is concealed as an active frame.

Tables needed by this document that are not in the manifest: none. The sizes 4 (SID) and 1 (untransmitted) of the frame lengths are scalar constants of this document, and PACKED_FRAME_BYTES holds only 24 and 20. The scalar constants 0.75 and 0.25 of the formant postfilter k1 smoothing, the tilt constant -0.25, the scaling gain constants 0.9375, 0.0625 and 1.0625, and the concealment constants 0.75, 521, 259 and 32768 are given in the text above.
