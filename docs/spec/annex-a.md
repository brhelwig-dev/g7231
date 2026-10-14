# Annex A: silence compression (VAD, SID frames, comfort noise)

This document specifies the voice activity detector (VAD), the comfort noise generator (CNG), the SID frame and the untransmitted frame of Annex A of ITU-T Recommendation G.723.1, as implemented by the floating-point reference of Annex B (version 5.2). It describes only what Annex A adds to or changes in the main body of the codec. For the procedures of the main body it refers to the other documents in this directory by topic: the encoder main path, the linear prediction analysis (including the Levinson-Durbin recursion and the sine detector), the line spectral pair (LSP) conversion and quantization, the fixed codebook, and the decoder.

## 0. Conventions and sources

### 0.1 Precedence

The Recommendation says (A.8) that where its text and the C source differ, the C source is correct. This document follows the source. Every place where the printed text of Annex A differs from the source is listed in section 10 and marked in the section that uses it.

### 0.2 Number formats

1. All real-valued state and all arithmetic in the reference are double precision (64-bit). The reference can be built for single precision only by a compile-time option that the conformance suite does not use. Use f64 everywhere.
2. Integer quantities are exact integers: the random generator state, the random draws, the counters of the VAD, the gain indices, the pitch lags and the loop counters. Where a division or shift of an integer is involved the document says "integer part" or "shifted right".
3. The first octet of a coded frame carries the frame type in its two least significant bits (section 5).
4. Frames have 240 samples, subframes have 60 samples. A "block" is a pair of subframes (120 samples): subframes 0 and 1 form block 0, subframes 2 and 3 form block 1.
5. Sums are formed by starting at 0.0 and adding the terms in increasing index order, unless the text says otherwise. The order matters only for the last bits of the result, but a few decisions (the VAD threshold, the Itakura distance) compare quantities that can differ in those bits.

### 0.3 Tables used

| Table | Use in this document |
|---|---|
| `VAD_THRESHOLD_SCALE` | VAD threshold (2.6) |
| `SID_GAIN_ENERGY_SCALES` | SID gain quantizer input (3.4). The value at index 0 is 0.008333, not exactly 1/120. The values are 0.008333, 61.6815, 30.84075 and 20.6145 |
| `SID_GAIN_SEGMENT_LIMITS` | SID gain quantizer (3.4). Values 1024, 9216, 115617 |
| `SID_GAIN_SEGMENT_BASES` | SID gain quantizer and decoder (3.4). Values 0, 32, 96 |
| `LSP_DC_VECTOR` | Decoder initialization (7.1) |
| `PITCH_GAIN_CODEBOOK_170` | Adaptive part of the comfort noise excitation (4.4). Columns 0 to 4 are the five tap gains |
| `TAMING_GAIN_BOUNDS_170` | Encoder taming update during inactive frames (3.8) |
| `MPMLQ_PULSE_COUNTS` | Number of pulses per subframe, 6, 5, 6, 5 (4.3) |
| `LSP_INTERPOLATION_WEIGHTS` | Interpolation of the SID LSP (3.7) |

No table is added by this document.

### 0.4 Frame types

The frame type of a frame is one of three values.

| Value | Meaning |
|---|---|
| 1 | Active speech frame (high rate or low rate) |
| 2 | SID frame (silence insertion descriptor, 4 octets) |
| 0 | Untransmitted frame (nothing is sent) |

The encoder decides the type; the decoder reads it from the bitstream.

### 0.5 Summary of the one decision that changes the results

Section 4.2 gives the random draw of the second pitch lag. The released version 5.2 source draws it from 19 values, and the README of the release says the test vectors were updated for that. The test vectors that are actually present in the Annex B package (the files with the dates of 1996) were generated with a draw from 21 values. When the 5.2 source is compiled and run on them, the encoder matches only 79.6 percent (low rate) and 73.6 percent (high rate) of the 10-frame groups of the VAD test sequences, and the decoder misses its SNR thresholds on several VAD streams. With 21 values the encoder output is bit-identical to the vectors and every VAD decoder output is identical to the vector (infinite SNR). Section 4.2 explains how to treat this. The other change of version 5.2 (the comparison in the filter distance, 3.3) has no effect on the encoder test sequences.

## 1. Overview of the encoder with VAD

For every frame the encoder performs these steps in this order. The steps marked "always" run for active and inactive frames. Their results are needed for the VAD or for the state that the next active frame inherits.

1. Read the coder rate of the frame (when a rate file is used). It matters only if the frame is active.
2. Remove the DC component with the high-pass filter of the main body (always).
3. Run the LPC analysis of the main body on the 240 samples and the 120 saved samples (always). It produces four sets of autocorrelation values, four LPC sets, and the sine detector history. At its end the four sets of autocorrelation values are summed and stored (section 3.1).
4. Convert the LPC set of subframe 3 to LSP (always, but the result is used only when the frame is active).
5. Run the VAD (section 2). Its inputs are the high-pass filtered samples of this frame before the input delay of step 7 is applied.
6. Quantize the LSP vector of subframe 3. The result is used only if the frame is active. The quantizer has no state, so for inactive frames this step may be skipped.
7. Shift the input delay memory (always). Apply the perceptual weighting filter to the delayed frame (always; the filter memories are updated in all frames).
8. Run the open-loop pitch estimator of the main body on the weighted speech, twice per frame (always). The two lags of the frame are stored into positions 2 and 3 of the VAD lag history (section 2.1) as they are found.
9. If the VAD decision is active (frame type 1), run the active path of the main body unchanged, then do the two assignments of section 3.9. If the VAD decision is inactive, run the inactive path of sections 3.2 to 3.8.
10. Pack the frame (section 5).

When VAD/CNG is disabled the encoder behaves exactly as the main body, and every frame is type 1.

## 2. Voice activity detector (A.2)

The reference function is called once per frame at step 5 of section 1.

### 2.1 Inputs and state

Inputs:

1. The 240 high-pass filtered samples s[0] to s[239] of the current frame, before the input delay. Only the samples s[50] to s[239] are read. The samples s[60] to s[239] (180 samples) are filtered and measured. The values are real numbers (not rounded to integers).
2. The sine flag of the current frame: true when the sine detector word has its top bit set after the LPC analysis of step 3. See 2.2.
3. The VAD lag history L[0] to L[3], explained below.
4. The noise filter c[0] to c[9].

State and initial values (set when the codec is initialized and, in the conformance encoder test, again every 10 frames):

| State | Initial value |
|---|---|
| Hangover counter H | 3 |
| Burst counter B | 0 |
| Previous filtered energy Penr | 1024.0 |
| Noise level Nlev | 1024.0 |
| Adaptation counter A | 0 |
| Lag history L[0], L[1], L[2], L[3] | 1, 1, 60, 60 |
| Noise filter c[0] to c[9] | all 0.0 |

The text (A.2.8) gives the same values and adds that everything else is zero.

The lag history holds open-loop pitch lags of whole frames. The pitch estimator yields two lags per frame, one for each half. The history is updated in two places:

1. At the end of the VAD function: L[0] is set to L[2] and L[1] is set to L[3].
2. At step 8 of section 1, after the VAD ran: L[2] is set to the first lag of the current frame and L[3] to the second.

So when the VAD of frame t runs, L[0] and L[1] are the two lags of frame t-2, and L[2] and L[3] are the two lags of frame t-1. The lags of the current frame are not yet known. The text (A.2.1) says "the preceding and the current frame"; the reference uses the two preceding frames. In the first frame after initialization the history is the initial values above. The lag values are integers as produced by the open-loop estimator (18 to 142).

### 2.2 Sine flag

This restates a part of the LPC analysis because the VAD depends on it exactly. The encoder-analysis document holds the full description.

1. For each of the four subframes of the frame, in order, the Levinson-Durbin recursion yields a value Q. Define Q = minus the second reflection coefficient (the coefficient found in the iteration with index 1, counting from 0), so Q = -k2. If the recursion stops early (see 3.2, the stopping rule) at any iteration, including iterations after the second, Q is set to 0.99 instead (this replaces a value found earlier in the same recursion).
2. The sine detector word W (16 bits) is shifted left by one bit for each subframe, bits that leave the top are lost, and then 1 is added to W if Q > 0.95 (strict).
3. After the four subframes, W is reduced to its low 15 bits. Let n be the number of bits that are 1 among those 15 bits. If n ≥ 14 the top bit (bit 15, value 32768) of W is set.
4. The sine flag is "bit 15 of W is set". It therefore says: at least 14 of the last 15 subframes had Q > 0.95.

The text writes this as "the second reflection coefficient ≥ 0.95". The reference uses the negated coefficient and a strict comparison, as in item 1 and item 2.

### 2.3 Steps

All quantities are double precision except the integers named.

1. Minimum lag. Lmin is the smallest of 145 and L[0] to L[3]. (The value 145 is the maximum lag of the codec. It never wins in practice.)
2. Match count. M is the number of pairs (i, j) with i from 0 to 3 and j from 1 to 8 for which the integer absolute value |j × Lmin - L[i]| is ≤ 3. Each lag may match several multiples only when Lmin is 6 or less, which happens in the first frame after initialization (the history 1, 1, 60, 60 gives M = 8). M is not clipped.
3. Adaptation counter. If M equals 4 exactly, or the sine flag is set, A = A + 2. Otherwise A = A - 1. Then A is limited to the range 0 to 6.
4. Inverse filter and energy. For n from 60 to 239 compute e[n] = s[n] - sum over j from 0 to 9 of c[j] × s[n-1-j]. The terms are subtracted from s[n] one at a time in increasing j. The raw energy is S = the sum of e[n]² for n = 60 to 239. Then Enr = 0.5 × (S ÷ 180). (Divide first, then halve.) The text (A.2.3) writes 1/80 of the sum; the reference differs by the factor 0.5 × 80 ÷ 180 and its thresholds are set for its own scale. The filter has the sign convention of a predictor: c[j] is the Levinson-Durbin output, not its negation.
5. Noise level update. Perform in this order:
   1. If Nlev > Penr then Nlev = 0.25 × Nlev + 0.75 × Penr.
   2. If A equals 0 then Nlev = Nlev × (33 ÷ 32). Otherwise Nlev = Nlev × (2047 ÷ 2048). The text writes 1.03125 and 0.9995; the reference constants are the exact fractions given here.
   3. Penr = Enr. (Penr now holds the energy of the current frame, for the next call.)
   4. If Nlev < 128.0 then Nlev = 128.0. If Nlev > 131071.0 then Nlev = 131071.0.
6. Threshold. Write Nlev = F × 2^E where E is an integer and F lies in [0.5, 1) (the usual frexp decomposition). Let T = (the integer part of F × 128) ÷ 64 - 1. T takes the values 0, 1/64, ..., 63/64, so only the six leading fraction bits of the noise level are used. Then
   Scale = (1 - T) × `VAD_THRESHOLD_SCALE`[18 - E] + T × `VAD_THRESHOLD_SCALE`[17 - E]
   Thresh = (Scale × Nlev) ÷ 4096.
   Because Nlev is between 128 and 131071, E lies between 8 and 17 and both indices are inside the table (indices 0 to 10). The text (A.2.5) gives the factor Thr as a logarithmic formula and compares Enr with Thr; the reference multiplies by Nlev as written here, and takes Thr from the table by linear interpolation. The table entry at index k is 4096 times the factor for the noise level 2^(17-k).
7. Raw decision. If Thresh > Enr (strict) the raw decision is inactive. Otherwise it is active. (The text writes "active if Enr ≥ Thr", which is the same.)
8. Counters, in this order:
   1. If the raw decision is active: B = B + 1 and H = H + 1. Otherwise B = B - 1, and if B < 0 then B = 0.
   2. If B ≥ 2: H = 6, and if also B ≥ 3 then B = 3.
   3. If H ≠ 0: the final decision is active, and if B equals 0 then H = H - 1. If H equals 0 the final decision is the raw decision (which can only be inactive at this point, because an active raw decision has incremented H).
9. Lag history: L[0] = L[2] and L[1] = L[3].
10. Return the final decision. Active becomes frame type 1. Inactive passes to the CNG (section 3), which produces type 0 or type 2.

The text (A.2.7) describes the hangover as 6 frames added to bursts of 2 or more frames. Step 8 is the exact rule.

### 2.4 The noise filter

The noise filter c is changed only by the CNG, in the SID computation (step S2 of 3.5). Between changes it keeps its value, also through active frames.

## 3. Encoder comfort noise generation (A.3 and A.4)

### 3.1 Autocorrelation history (A.4.1)

State: a history of four sets R_k[j], k = 0 to 3, j = 0 to 10. R_0 is the current frame and R_1 to R_3 are the three preceding frames. All values start at 0.0.

At the end of the LPC analysis of every frame, active or not, the history is shifted: R_3 = R_2, R_2 = R_1, R_1 = R_0. Then R_0[j] is set to the sum, starting from 0.0 and adding in subframe order 0, 1, 2, 3, of the autocorrelation values of the four subframes at lag j. The subframe values are the ones of the main body (clause 2.4): with the white-noise correction factor 1025/1024 on lag 0, the binomial lag window on lags 1 to 10, and the division by 180 × 180. A subframe whose lag 0 value is exactly 0.0 has all its lags set to 0.0.

### 3.2 Levinson-Durbin with returned energy

The CNG uses the recursion of the main body, but also uses the final prediction error energy it returns. Inputs are the autocorrelation values r[0] to r[10]. Output: ten coefficients a[0] to a[9] (predictor convention, prediction of x[n] is the sum of a[j] × x[n-1-j]) and the energy.

The recursion runs for i from 0 to 9. Let Err start as r[0]. At step i compute N = r[i+1] - sum over j < i of a[j] × r[i-j]. If |N| ≥ Err, stop at once: keep the coefficients found so far (those with higher index stay 0.0) and return the current Err. Otherwise k = N ÷ Err, Err = Err - N × k, update the coefficients in the usual way, and continue. The function returns the Err of the point where the loop ended. When r[0] is 0.0 the first step stops at once and the result is ten zeros and energy 0.0.

The stopping rule is the same one that sets Q = 0.99 for the sine detector (2.2).

### 3.3 Frame type decision (A.4.2)

This runs for every inactive frame. The state used:

| State | Initial value |
|---|---|
| Previous frame type P | 1 |
| Energy list E[0], E[1], E[2] | not initialized; written before use |
| Energy count NE | not initialized; written before use |
| Reference gain index IRef | not initialized; written before use |
| Reference function RC[0] to RC[10] | not initialized; written before use |
| SID gain G_sid | not initialized; written before use |
| SID LSP vector | not initialized; written before use |
| SID filter | 0.0 for all ten coefficients |
| Smoothed gain G_cur | 0.0 |
| Random seed | 12345 |

The values "not initialized" are in the reference static storage and so zero, but they are always assigned in the first inactive frame (which is always a SID frame) before any use. Do not rely on them.

Steps:

1. Shift the energy list: E[2] = E[1], E[1] = E[0].
2. Run the recursion of 3.2 on the autocorrelation values R_0[0] to R_0[10] of the current frame. This gives the current filter a_cur[0..9] and the energy, which is stored in E[0].
3. If P equals 1 (the previous frame was active): the frame type is 2 (SID), NE = 1, and the quantized gain index is q = SIDQUANT(NE) as defined in 3.4.
4. Otherwise: NE = NE + 1, and if NE > 3 then NE = 3. q = SIDQUANT(NE). Then decide:
   1. Evaluate the filter distance test DIFF(RC, R_0, E[0]) of the next paragraph, with the reference function RC stored at the last SID frame. If it says "different", the frame type is 2.
   2. Otherwise, if the integer absolute value |q - IRef| is greater than 3 (strict), the frame type is 2.
   3. Otherwise the frame type is 0 (untransmitted).

Filter distance test DIFF(RC, R, Eng): compute D = sum over j from 0 to 10 of RC[j] × R[j], and the limit 1.2136 × Eng. The filters are "similar" if D ≤ limit, and "different" if D > limit. Version 5.2 changed the comparison from "<" to "≤" for "similar". This agrees with the printed text A-10, which calls the filters different when the sum exceeds E × thr1 with thr1 = 1.2136. The change had no effect on the encoder test sequences.

Reference function RC from a filter a[0..9] (the quantity of A-11): let u[0] = 1 and u[k] = -a[k-1] for k = 1 to 10. Then
RC[0] = sum over k from 0 to 10 of u[k]²
RC[j] = 2 × sum over k from 0 to 10-j of u[k] × u[k+j], for j = 1 to 10.
The reference computes RC[0] as 1.0 plus the sum of a[j]², and RC[j] as 2 × (-a[j-1] + the sum over m from 0 to 9-j of a[m] × a[m+j]). This is the same quantity.

### 3.4 Gain quantization (A.4.3)

SIDQUANT(n) with n from 1 to 3 (or the variant with n = 0 of 6.4) is `Qua_SidGain`. Its input is a scaled energy x and its output an index from 0 to 63.

1. For n from 1 to 3: x = `SID_GAIN_ENERGY_SCALES`[n] × (E[0] + E[1] + ... + E[n-1]), summed in increasing index. For n = 0 (decoder only, 6.4): x = `SID_GAIN_ENERGY_SCALES`[0] × (the stored energy).
2. If x ≥ 115617 (`SID_GAIN_SEGMENT_LIMITS`[2]) the index is 63. Stop.
3. Choose the segment. If x ≥ 9216 (`SID_GAIN_SEGMENT_LIMITS`[1]): segment s = 2 and the search depth d = 4. Otherwise d = 3, and s = 1 if x ≥ 1024 (`SID_GAIN_SEGMENT_LIMITS`[0]), else s = 0.
4. The levels of segment s are g(s, j) = `SID_GAIN_SEGMENT_BASES`[s] + j × 2^(s+1), for j = 0 to 15 in segments 0 and 1 and j = 0 to 31 in segment 2. Index = 16 × s + j. So the gains are 0, 2, ..., 30 (indices 0 to 15), 32, 36, ..., 92 (16 to 31) and 96, 104, ..., 344 (32 to 63). Segments join without a gap.
5. Binary search. Start with the integer J = 2^d (8 or 16) and step K = J ÷ 2. Repeat d times: P = g(s, J)². If x ≥ P then J = J + K, else J = J - K. Then K = K ÷ 2 (integer). After d rounds J is odd, between 1 and 15 (segments 0 and 1) or 1 and 31 (segment 2).
6. Final choice by the distance in the energy domain. Let Y = g(s, J)² - x.
   1. If Y ≤ 0: let J2 = J + 1 and Z = x - g(s, J2)². If Y > Z (strict) the segment index is J, otherwise J2.
   2. If Y > 0: let J2 = J - 1 and Z = x - g(s, J2)². If Y < Z (strict) the segment index is J, otherwise J2.
   The result is 16 × s + the chosen index. J2 may leave the segment by one (for example J = 15 in segment 0 gives J2 = 16, which is level 32 = the first level of segment 1; the formula for the level still holds). Ties go to the higher level when Y ≤ 0 and to the lower level when Y > 0.
7. Examples, for checking: x = 2.7314 gives 1; x = 1100 gives 16; x = 1400 gives 17; x = 1500 gives 18; x = 2.0 gives 0; x = 3.0 gives 1; x = 9216 gives 32; x = 115617 gives 63.

The text (A.4.3) writes the input as G = α_w × sqrt(E ÷ (kE × 240)) with α_w = 2.70375 and bounded to [0, 352], and compares G with the levels. The reference folds α_w, the division and the squaring into the scale factors: it compares the scaled energy x with the squared levels, and `SID_GAIN_ENERGY_SCALES`[n] is the factor for n frames. No square root is taken in the quantizer.

Decoding of an index q (`Dec_SidGain`): s = q shifted right by 4 (integer part of q ÷ 16). If s equals 3 then s = 2. j = q - 16 × s. The gain is `SID_GAIN_SEGMENT_BASES`[s] + j × 2^(s+1). This is a real number between 0 and 344. It is the root mean square amplitude of the comfort noise excitation per sample (4.5).

### 3.5 SID filter, LSP quantization and gain (A.4.4)

This runs only when the frame type is 2 (steps S1 to S6).

S1. Average past filter. Form r_p[j] = R_1[j] + R_2[j] + R_3[j] for j = 0 to 10 (the three frames before the current one, summed in this order, the current frame excluded). Run the recursion of 3.2 on r_p. The coefficients are a_p. The energy returned is not used.

S2. If the VAD adaptation counter A (2.3) equals 0, set the noise filter c of the VAD to a_p. This uses the value of A computed in the current frame. It happens before the replacement of step S4.

S3. Compute RC from a_p as in 3.3 and store it as the reference function.

S4. Evaluate DIFF(RC, R_0, E[0]) with the RC of step S3. If the result is "different", the SID filter is a_cur and the reference function is recomputed from a_cur. Otherwise the SID filter is a_p.

S5. Convert the SID filter to LSP with the conversion `AtoLsp` of the main body (it applies the 0.994 bandwidth expansion itself), using the quantized LSP vector of the previous frame as the fall-back that the conversion returns when fewer than ten roots are found. Quantize the result with the 24-bit LSP quantizer (`Lsp_Qnt`, predictor from the quantized LSP vector of the previous frame) and obtain the quantized vector with `Lsp_Inq` using the 24-bit index and the erasure flag 0. The result is the SID LSP vector. The 24-bit index goes into the SID frame (section 5).

S6. The gain index of the SID frame is q from 3.3. Store IRef = q. Store G_sid = the decoded gain of q (3.4).

The previous quantized LSP vector is the vector that the previous frame left as the LSP state of the encoder: the decoded vector of the last active frame, or the SID LSP vector of the previous inactive frame.

### 3.6 Excitation of the frame (A.4.5)

Done for every inactive frame, SID or untransmitted.

1. If P equals 1: G_cur = G_sid. Otherwise G_cur = 0.875 × G_cur + 0.125 × G_sid. (The text A-18 has the same weights, conditioned on the previous VAD decision.)
2. The excitation of the 240 samples is generated by the procedure of section 4 from G_cur, the pitch memory of the encoder (the 145 past excitation samples) and the encoder's random seed. The procedure also fixes the pitch lags and gain indices (random values) and updates the pitch memory.

For frames of type 0, G_sid and the SID LSP vector keep the values of the last SID frame.

### 3.7 LSP interpolation and update (A.4.6)

1. The interpolated LSP vectors of the four subframes are formed from the previous quantized LSP vector and the SID LSP vector with the weights `LSP_INTERPOLATION_WEIGHTS` (0.25, 0.5, 0.75, 1.0 for the weight of the new vector), and converted to LPC sets for the synthesis filter (the interpolation of the main body, clause 2.7). The previous vector is the one named at the end of 3.5.
2. The previous quantized LSP vector of the encoder becomes the SID LSP vector.
3. P = the frame type of this frame (0 or 2).

### 3.8 Encoder state during inactive frames

The decoder will use the SID parameters and the random excitation. The encoder's main path must therefore end each inactive frame in the same state as if it had coded the frame itself with the comfort noise excitation. The following is done in this order after the type decision of 3.3 to 3.7.

1. Before the excitation overwrites the frame buffer: copy the last 145 samples of the weighted speech of the current frame (the frame buffer at step 7 of section 1, samples 95 to 239) to the encoder's weighted speech memory. This is the same data the active path stores.
2. The excitation of 3.6 is placed in the frame buffer (240 values).
3. For each subframe i from 0 to 3 in order:
   1. Update the taming error memory with the procedure of the main body (the error-energy tracking used by the closed-loop pitch search). Its arguments are the lag centre Olp of the block (the first lag for subframes 0 and 1, the second lag for subframes 2 and 3), the lag offset code of the subframe (1, 0, 1, 3) and the gain index of the subframe, all from section 4. The memory has 5 entries, initially 3.81464e-6 each. The 170-entry table `TAMING_GAIN_BOUNDS_170` applies at both rates, because every comfort noise Olp is at least 123 (the main body takes the 85-entry table only for the high rate and Olp below 58). Let Lag = Olp - 1 + offset code. It lies between 122 and 143 (145 with M = 21), so its integer part Lag × 1092 shifted right by 15 equals 4 and 150 differs from Lag. In that case the update is:
      1. beta = `TAMING_GAIN_BOUNDS_170`[gain index].
      2. w = memory[3] × beta + 3.81464e-6, W0 = memory[2] × beta + 3.81464e-6, W1 = memory[4] × beta + 3.81464e-6.
      3. If w > W0 then W0 = w. If w > W1 then W1 = w.
      4. Limit W0 and W1 above to 256.0.
      5. memory[4] = memory[2], memory[3] = memory[1], memory[2] = memory[0], memory[0] = W0, memory[1] = W1.
      The general rule for other lags belongs to the encoder analysis. Only this case occurs here.
   2. Update the ringing memories (clause 2.19) with the excitation of the subframe, the interpolated quantized LPC set of the subframe from 3.7, and the perceptual weighting filter coefficients of the subframe that the LPC analysis of this frame produced (the weighting filter built from the unquantized LPC set, as in the active path). This updates the weighting filter ringing memories and the 145-value memory of the harmonic noise shaping filter. The harmonic noise shaping parameters are not computed in inactive frames, and the update does not use them.
4. The pitch memory (the 145 past excitation samples) was already updated by the excitation procedure (section 4.6).
5. The LSP memory was updated in 3.7. The high-pass filter memory, the input delay memory, the perceptual weighting filter memories and the sine detector word were updated by the steps marked "always" in section 1.

The first active frame after an inactive stretch then runs the unchanged main path from this state.

### 3.9 Back to active frames

When a frame is active, after the main path the encoder sets P = 1 and the random seed = 12345. Nothing else of the CNG is changed (the autocorrelation history, the noise filter and the VAD state are maintained as described in 3.1 and section 2). Consequently every run of inactive frames starts with the seed 12345 and with a SID frame.

## 4. Comfort noise excitation, shared by encoder and decoder (A.4.5 and A.5)

The same procedure runs in the encoder (3.6) and in the decoder (6.4). Inputs: the target gain G (G_cur), the 145 sample pitch memory H (index 0 is the oldest sample, 144 the newest), the random seed state. Outputs: 240 excitation samples, the new pitch memory, the new seed state and the parameters in 4.2.

### 4.1 Random number generator

The state r is an unsigned 16-bit integer. The recurrence is r = (r × 521 + 259) modulo 65536. A draw of "random value in 0 to n-1" (call it DRAW(n)) does the following: advance the state, take the low 15 bits V of the new state (V = state AND 32767), and return the integer part of V × n ÷ 32768 (an integer product shifted right by 15). The result is in the range 0 to n-1.

The same recurrence is used by the erasure concealment of the decoder (clause 3.10.2), with a separate state variable. The CNG state and the concealment state do not interact.

The seed is 12345 at initialization and after every active frame (3.9, 6.5). The first states are 9476, 22055, 22114, 52853 (low 15 bits 20085) and 11552.

### 4.2 Parameters (random draws)

All draws are made first and in this order, before any excitation is computed:

1. First lag: Olp0 = 123 + DRAW(21). The range is 123 to 143.
2. Second lag: Olp1 = 123 + DRAW(M), with M explained next.
3. Gain indices of the four subframes: AcGn[s] = 1 + DRAW(50) for s = 0, 1, 2, 3 in order. The range is 1 to 50. (The text says 0 to 49. The reference adds 1, so row 0 of the codebook is never used.)
4. Grid and sign words, one per block: W_b = DRAW(8192) for b = 0, then b = 1.
5. Pulse positions: see 4.3.

Lag offset codes (fixed, not drawn): AcLg[0] = 1, AcLg[1] = 0, AcLg[2] = 1, AcLg[3] = 3. They enter the lag of each subframe as Lag[s] = Olp(block of s) - 1 + AcLg[s]. So the lags are Olp0, Olp0 - 1, Olp1, Olp1 + 2.

The value of M. The released 5.2 source has M = 19, so Olp1 lies in 123 to 141 and Lag[3] is at most 143. The earlier version had M = 21, so Olp1 up to 143 and Lag[3] up to 145. The sources do not state the reason for the change. The test vectors of the package were generated with M = 21. Which to use:

1. M = 21 reproduces the package vectors: encoder 100 percent of the 10-frame groups, and every decoder SNR infinite, on all the VAD vectors (tst53v, tst63v, in5, dm29, dmbis, read7, test16, vad6353, pathc53, pathc63h, mixed rate with erasures).
2. M = 19, the released source, gives 79.6 percent and 73.6 percent of the groups for the two encoder tests, which is below the 95 percent threshold, and SNR of 26 to 55 dB on several decoder streams (dmbis 54 dB against a threshold of 62, read7 26 dB against 58, test16 38 to 41 dB against 60, vad6353 31 to 33 dB against 57), while others still pass.
3. Only the value of Olp1 differs between the two. The count of draws is the same, so no other value changes in the draw sequence.

An implementation that has to pass the package vectors uses M = 21. Implement M as one constant. If the conformance vectors are regenerated from the 5.2 source, M = 19 applies. The decision belongs to the project (section 11).

### 4.3 Grids, signs and positions

For block b (0 or 1) take the 13-bit word W_b. Bit 0 of W_b is the grid g_a of the first subframe of the block (0 or 1). Bit 1 is the grid g_b of the second subframe. Bits 2 to 12 give the signs of the 11 pulses of the block, sign number k (k = 0 to 10) is +1 if bit 2+k is 1 and -1 if it is 0. Subframe offsets within the block: offset of the first subframe = g_a, offset of the second subframe = 60 + g_b.

Positions, for s from 0 to 3 in order. The subframe has `MPMLQ_PULSE_COUNTS`[s] pulses (6, 5, 6, 5). Start with the list of the integers 0 to 29 and m = 30. For each pulse of the subframe in order: j = DRAW(m), position = 2 × list[j] + offset of subframe s (the offset is relative to the start of the block), then m = m - 1 and list[j] = list[m]. So the 11 positions of a block are distinct and lie in 0 to 119. The first six positions (subframe 0) all have the grid parity of that subframe.

The positions of a block, in order of drawing, are paired with the signs of the block in the same order: the first pulse of the first subframe gets sign number 0, and so on up to sign 10 for the last pulse of the second subframe.

Check values for the first inactive period after initialization (seed 12345): Olp0 = 129, Olp1 = 135 with M = 19 and 137 with M = 21 (the draw value is 22055), AcGn = 34, 31, 18, 35. W_0 = 5583 and W_1 = 908. Offsets of the four subframes: 1, 61, 0, 60. Block 0 signs: +1 +1 -1 -1 +1 +1 +1 -1 +1 -1 +1. Block 1 signs: +1 +1 -1 -1 -1 +1 +1 +1 -1 -1 -1. Positions in drawing order: 47 51 27 41 21 9 | 117 83 69 81 109 | 54 52 24 20 10 58 | 72 66 68 104 86. The state after all draws is 17547. These values (with M = 19) were checked against a run of the reference that printed them. Only Olp1 changes with M.

### 4.4 Adaptive part

Handle the blocks in order, b = 0 then 1. The pitch memory H seen by block 1 includes the excitation of block 0 (4.6). For the two subframes of a block, t = 0 and 1, let o = 60 × t, let L be the lag of that subframe (4.2) and AcGn its gain index (4.2). The adaptive vector d of 60 samples is

d[m] = sum over k from 0 to 4 of `PITCH_GAIN_CODEBOOK_170`[AcGn][k] × H[143 + o - L + m + k], for m = 0 to 59.

All indices are between 0 and 144. The second subframe of a block reads from the same memory H as the first, displaced by o = 60. It does not see the samples of the first subframe of the block. This makes no difference to a continuous memory, because every lag is at least 122: the largest index read is 266 - L, which is at most 144, so the reads never reach the new samples. The formula is the five-tap adaptive excitation of the main body (clause 3.4); for lags of 62 or more its periodic repetition of the last L samples never takes effect, which gives the formula above. The lags are Olp0, Olp0 - 1, Olp1 and Olp1 + 2, so they lie between 122 and 143 (up to 145 with M = 21).

The two vectors d of the block are concatenated to the 120-sample block vector u[0] to u[119]. The codebook table is the 170-entry table at both rates: the main body takes the 85-entry table only for the high rate and lags below 58.

### 4.5 Fixed part and gain

For block b:

1. The pulse vector v has 120 samples, all 0.0, and the value sign_k at the position p_k for k = 0 to 10 (4.3).
2. Eu = sum over n from 0 to 119 of u[n]². Cuv = sum over k from 0 to 10 of u[p_k] × sign_k (summed in increasing k).
3. The pulse energy of v is 11 (all positions distinct, sign squared is one). The aim is that the energy of u + X v equals 120 × G². This gives the quadratic 11 X² + 2 Cuv X + (Eu - 120 G²) = 0. With the reciprocal R11 = 1.0 ÷ 11.0 (computed once in double precision, and used as a multiplier, not a divisor):
   b0 = Cuv × R11
   c = (Eu - (G × G) × 120.0) × R11
   delta = b0 × b0 - c.
4. If delta ≤ 0 then X = -b0. Otherwise q = sqrt(delta), x1 = -b0 + q, x2 = b0 + q, and X = x1, except that if |x2| < |x1| (strict) then X = -x2. This chooses the root of smaller magnitude; when b0 is 0 the positive root x1 is kept.
5. Limit X to the range -5000.0 to 5000.0. (The text says only X ≤ 5000. The reference limits both ends.)
6. e[n] = u[n] for all n, then for k from 0 to 10 add X × sign_k to e[p_k].
7. Clipping to the 16-bit range: for each n from 0 to 119, if e[n] > 32766.5 then e[n] = 32767.0, and else if e[n] < -32767.5 then e[n] = -32768.0. Other values are left as they are (no rounding to integer).

### 4.6 Memory update

After each block, the memory H shifts down by 120: H[i] = H[i+120] for i = 0 to 24, then H[25 + i] = e[i] for i = 0 to 119 (the clipped values). Block 1 is computed after this update. The 240 output samples are the two blocks e, in order.

## 5. SID frame format and the untransmitted frame (A.6, Table A.1)

The frame type is the integer made of the two least significant bits of the first octet, bit 0 being the least significant:

| Value of the two bits | Meaning | Octets |
|---|---|---|
| 0 | Active, high rate (6.3 kbit/s) | 24 |
| 1 | Active, low rate (5.3 kbit/s) | 20 |
| 2 | SID | 4 |
| 3 | Untransmitted | 0 on a channel; 1 octet in the files of the conformance suite |

SID frame (32 bits, Table A.1): all fields are written least significant bit first, from bit 0 of octet 1.

1. Bits 0 to 1 of octet 1: the type, the value 2 (the least significant bit is 0, the next is 1). In the notation of the Table these are RATEFLAG_B0 and VADFLAG_B0.
2. The 24-bit LSP index (`Lsp_Qnt` result, band 0 in bits 23 to 16 of the integer, band 1 in bits 15 to 8, band 2 in bits 7 to 0), written from its bit 0 (LPC_B0) to bit 23 (LPC_B23). Octet 1 bits 2 to 7 hold LPC_B0 to LPC_B5, octet 2 holds LPC_B6 to LPC_B13, octet 3 holds LPC_B14 to LPC_B21, and octet 4 bits 0 and 1 hold LPC_B22 and LPC_B23.
3. The 6-bit gain index (3.4) from GAIN_B0 to GAIN_B5, in octet 4 bits 2 to 7.

Example: the first SID frame of the test sequence tst63v is the octets c6 12 22 07. The type bits are 2, the LSP index is 0xc884b1 (band indices 0xc8, 0x84, 0xb1) and the gain index is 1.

Untransmitted frame: no information is sent. The reference files of the conformance suite hold one octet whose two low bits are 3 and whose other bits are 0 (the value 3). The decoder reads one octet, finds the type 3, and treats the frame as untransmitted without reading more. An implementation that works on files of the suite must read and write that octet. On a real channel nothing is sent.

The sizes in the table decide how many octets to read after the first one: 23 more for type 0, 19 more for type 1, 3 more for type 2, none for type 3.

For active frames the first two bits also say the rate, as in the main body. The 24-bit index and the 6-bit gain field are all that the SID frame carries.

## 6. Decoder comfort noise generation (A.5)

### 6.1 State

| State | Initial value |
|---|---|
| Previous frame type P_d | 1 |
| SID gain G_sid | 0.0 |
| SID LSP vector | `LSP_DC_VECTOR` |
| Smoothed gain G_cur | 0.0 (always assigned before use) |
| Random seed | 12345 |

The decoder initializes these always (not only with VAD on). The decoder state of the main body (previous LSP vector, 145 past excitation samples, the synthesis and postfilter memories) is initialized as in the main body.

### 6.2 Frame type of the frame being decoded

1. Read the type from the first octet (section 5). If the frame is flagged as erased, or its active-frame fields are invalid (the same checks the main body applies: a lag code above 123 or a gain index outside its range, which the main body also treats as an erasure), the type is replaced by the rule of A.5.2 (A-21): if P_d equals 1 the frame is treated as active (type 1) with the erasure flag, and otherwise it is treated as untransmitted (type 0). Nothing of the contents of the erased frame is used in the second case.
2. P_d is set to the final type of the frame after the frame has been processed (6.6).

### 6.3 SID frame (type 2)

1. G_sid = the decoded gain (3.4) of the 6-bit index of the frame.
2. The SID LSP vector = `Lsp_Inq` of the 24-bit index, the previous quantized LSP vector of the decoder and the erasure flag 0.

### 6.4 Untransmitted frame (type 0), and common processing

For type 0 nothing is decoded. If P_d equals 1 (the previous frame was active, so the SID frame that should have preceded was lost, or the stream contains no SID frame), the SID information is rebuilt from the last active frame:

1. The SID LSP vector is the quantized LSP vector of the last good active frame (6.5).
2. G_sid = the decoded gain (3.4) of the index SIDQUANT(0) (the n = 0 variant, 3.4, step 1), where the stored energy is the quantity saved by the last good active frame (6.5). Both the quantization and the decoding are used, so G_sid is a table level.

For type 0 with P_d not equal to 1, G_sid and the SID LSP vector keep their values.

Then for both type 0 and type 2:

1. G_cur = G_sid if P_d equals 1, and otherwise G_cur = 0.875 × G_cur + 0.125 × G_sid. (An erased frame after a SID frame is a type 0 frame, so it continues the smoothing.)
2. The 240 excitation samples are generated by the procedure of section 4 from G_cur, the decoder's 145 sample excitation memory and the decoder's random seed. The memory is updated by that procedure. (Compare with the encoder, where the same procedure updates the encoder's own memory.)
3. The four interpolated LPC sets are formed from the previous quantized LSP vector of the decoder and the SID LSP vector, as in 3.7.
4. The previous quantized LSP vector of the decoder becomes the SID LSP vector.

### 6.5 What an active frame leaves for the comfort noise

In an active frame the decoder of the main body runs unchanged and in addition:

1. If the frame was not erased (the erasure count is 0 after the update of the main body): the quantity saved for the lost-SID case is the sum of the squares of the last 120 samples of the frame's reconstructed excitation (adaptive plus fixed contribution, before the clipping to the 16-bit range and before the pitch postfilter), taken at the point where the main body computes its voicing classification (clause 3.10.2). The same sum is saved in the variable that holds G_sid; a later SID frame overwrites it with its gain. Also the SID LSP vector is set to the frame's quantized LSP vector (the vector that becomes the previous LSP vector).
2. If the frame was erased and regenerated by the concealment of the main body, neither the saved energy nor the SID LSP vector is changed.
3. At the end of every active frame, erased or not, the random seed is reset to 12345.

### 6.6 Synthesis, postfilter, state at the end of the frame

1. For comfort noise frames the excitation of 6.4 goes to the synthesis filter subframe by subframe with the interpolated LPC sets. The pitch postfilter of clause 3.6 is not applied (the text, A.1, says it is switched off). The formant postfilter, the tilt compensation and the gain scaling of clauses 3.8 and 3.9 are applied exactly as for active frames, and their memories and the gain scaling state continue across frames of all types. When the postfilter option is off, neither part runs.
2. The synthesis filter memory and the postfilter memories are updated as in active frames.
3. The erasure count, the interpolation gain and index of the concealment, and the concealment random seed are not changed by comfort noise frames.
4. P_d = the type of the frame (0, 1 or 2).

The first active frame after comfort noise frames runs the main decoder with the excitation memory, the previous LSP vector, the synthesis memory and the postfilter memories as left by the comfort noise frames. The erasure count that it starts from is the one left by the last active frame, and is set to 0 if the frame is good.

### 6.7 Erasure interplay (A.5.2)

| Previous frame type | Current frame erased | Result |
|---|---|---|
| 1 (active) | yes | Decoded as active with the concealment of the main body |
| 0 or 2 | yes | Decoded as untransmitted: the SID parameters stay, the gain smoothing and the random generator continue |
| 1 (active) | no, type 0 | The lost-SID case of 6.4 |

The lost-SID case also arises when erased frames come between the last good active frame and the untransmitted frame: each erased frame is treated as active (concealed), P_d stays 1, and the first good frame that is type 0 rebuilds the SID information from the last good active frame (its LSP vector and energy, 6.5 items 1 and 2).

## 7. Initialization (A.2.8, A.4.7, A.5.3)

### 7.1 Summary

| Item | Value or section | Applies to |
|---|---|---|
| VAD state | 2.1 | encoder, only with VAD on |
| Autocorrelation history R_0 to R_3 | all 0.0 | encoder, only with VAD on |
| SID filter | 0.0 | encoder |
| Smoothed gain (encoder) | 0.0 | encoder |
| P (encoder) | 1 | encoder |
| Random seed (encoder) | 12345 | encoder |
| P_d, G_sid, SID LSP, seed (decoder) | 6.1 | decoder, always |

In the conformance encoder test the whole codec (main body, VAD and CNG, encoder and decoder) is re-initialized every 10 frames (the option -R10). Initial values then matter in every group: the VAD starts with H = 3, so the first three frames of a group are active; the first inactive frame of a group is a SID frame. The energy list, the energy count, IRef, RC, G_sid and the SID LSP vector of the encoder are not re-initialized by the reference; this is harmless because they are written in the first SID frame before they are read.

### 7.2 Check values for the VAD

The reference run on tst63v.bin (high rate, -v, -R10), first 10 frames, shows the following. The digits after the sixth may differ on other computers.

| Frame | L[0..3] before the call | M | Enr | Nlev after update | Thresh | Raw decision | B | H | Final | Type sent |
|---|---|---|---|---|---|---|---|---|---|---|
| 0 | 1 1 60 60 | 8 | 1.967456 | 1056.000000 | 3734.0596 | inactive | 0 | 2 | active | 1 |
| 1 | 60 60 18 56 | 2 | 6.363992 | 273.771704 | 1214.5695 | inactive | 0 | 1 | active | 1 |
| 2 | 18 56 57 59 | 3 | 0.908135 | 128.000000 | 641.5313 | inactive | 0 | 0 | active | 1 |
| 3 | 57 59 20 28 | 3 | 3.214495 | 128.000000 | 641.5313 | inactive | 0 | 0 | inactive | SID |
| 4 | 20 28 30 93 | 1 | 2.432036 | 128.000000 | 641.5313 | inactive | 0 | 0 | inactive | 0 |

(The fifth column of the first frame follows from the initial value 1024 × 33/32 = 1056.) In frame 3 the energy E[0] is 0.044282, the count NE is 1, the gain index is 1, the type is 2, and the SID frame octets are c6 12 22 07. In frame 4, E[0] = 0.054993, NE = 2, q = 1 and the type is 0.

## 8. Mixed-rate operation

The coder rate is chosen per frame (a rate file in the encoder, the first two bits of the frame in the decoder). With VAD on:

1. The rate of an inactive frame has no effect on any computation. The SID frame and the untransmitted frame have no rate field of their own: the SID type value 2 and the untransmitted type value 3 replace the two rate bits.
2. The comfort noise excitation uses the 170-entry pitch gain codebook at both rates (4.4) and the taming memory update uses the 170-entry bound table at both rates (3.8). So a rate change does not alter the comfort noise.
3. In the encoder, the rate is read for each frame, including inactive ones, but only the active path uses it. In the decoder, the rate of the last active frame stays selected through comfort noise frames, but is not used until an active frame replaces it with its own rate bits.
4. A rate change between two active frames separated by inactive frames needs no special handling: the state handed over (excitation memory, ringing memories, taming memory, LSP memory, weighted speech memory) does not depend on the rate.
5. The VAD, the filter test and the SID gain do not depend on the rate.

## 9. Floating-point notes

1. The reference is built for double precision. The decisions most sensitive to rounding are the VAD threshold comparison, the filter distance test, the integer search inside the SID gain quantizer, and the choice of the root in 4.5. Follow the order of operations given in the steps.
2. The reference was compiled and run on a 64-bit ARM computer, a different processor from the one that generated the vectors in 1996. With M = 21, with optimization off and fused multiply-add contraction off, the encoder output for tst53v and tst63v was bit-identical to the vectors and every VAD decoder output was identical to the vector (infinite SNR). The two encoder sequences and one decoder stream (dmbis) were repeated with optimization on and contraction at the compiler default, with the same result. So the procedures here do not depend on the rounding details of a particular processor.
3. The integer state stays small: H within 0 to 7, B within 0 to 3, A within 0 to 6.

## 10. Where the text and the reference differ

| Topic | Text | Reference (followed) |
|---|---|---|
| Lags used for voicing in the VAD | Preceding and current frame | The two preceding frames (2.1) |
| Voiced test | Count pc in the range 1 to 4, voiced when 4 | Count of matches over 8 multiples, equal to 4 exactly, not limited (2.3) |
| Sine detector | Second reflection coefficient at least 0.95 | Negated coefficient strictly above 0.95, and 0.99 on early stop (2.2) |
| Filtered energy | Sum divided by 80 | Sum divided by 180, halved (2.3) |
| Noise level decay | Factor 0.9995 | Factor 2047/2048 (2.3) |
| Threshold | Factor Thr in a formula, compared with Enr | Table interpolation over six mantissa bits, multiplied by Nlev, compared with Enr (2.3) |
| Hangover | 6 frames after bursts of 2 or more | Exact counter rule of 2.3 step 8 |
| Filter distance | Different if the sum exceeds E × 1.2136 | The same since version 5.2: similar if not greater (3.3) |
| Gain quantizer input | α_w × sqrt of mean energy, bounded to 352 | Scaled energy against squared levels with `SID_GAIN_ENERGY_SCALES` (3.4) |
| Gain indices of the comfort noise | 0 to 49 | 1 to 50 (4.2) |
| Second lag range | 123 to 143 for each lag | First 123 to 143, second 123 to 141 in 5.2 (4.2); the vectors need 123 to 143 |
| Fixed gain limit | At most 5000 | Between -5000 and 5000 (4.5) |
| Frame erasure after SID | As in A-21 | The same, with the invalid-field case added (6.2) |

## 11. Open points

1. The choice of M in 4.2 (19 or 21) is a decision for the project. The measurements are in 4.2.
2. The README of the release states that the vectors named in its list of modified vectors were updated for version 5.2. The package files carry the date of 1996 and give the results of M = 21. If another set of vectors exists, M = 19 may match it; this was not available.
