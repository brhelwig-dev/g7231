# G.723.1 encoder: analysis, short-term and long-term prediction (VAD disabled)

This document specifies, for the encoder with voice activity detection and comfort noise generation disabled, everything that happens between the input samples and the two fixed codebook searches. It covers clauses 2.2 to 2.14, 2.18, 2.19 and 2.21 of the Recommendation, together with the encoder-only error-tracking procedure ("taming") that the Recommendation does not describe. The LSP inverse quantizer, the LSP interpolation and the LSP to LPC conversion (clauses 2.6, 2.7, 3.2, 3.3 and 3.10.1) are specified here once and are shared with the decoder, as are the residual extraction and adaptive codebook decoding of section 9.3.

The searches of the two fixed codebooks (clauses 2.15 and 2.16) are specified in the fixed codebook document. Section 10 states where they are called and what they exchange with the steps below. Bit packing is specified in the bitstream document. Section 11 states the one packing rule that this document must agree on, the 12-bit GAIN parameter.

Where the Recommendation text and the reference software differ, the reference is what the conformance tests measure. Each difference is marked "The text does not say" or "The reference differs", and section 13 collects them.

## 1. Numeric model

1. Every real quantity of the reference is a 64-bit floating point number. The reference declares its real type once, as double precision. A different (single precision) type is selected only by a build macro that the source does not set by default, and no build file is supplied in the source directory of the release. This document assumes the default, double precision, and implements every real value as f64. No computation on the analysis path is done in single precision.
2. Input samples are the 16-bit PCM values converted to real numbers with no scaling. There is no division by 32768 anywhere in the encoder. The high-pass filter output, the weighted speech and the excitation keep the 16-bit amplitude scale and are never rounded to integers. The only clipping of a real signal is the excitation history clip of section 9.5.
3. A "dot product of length N" of two vectors a and b means: start from the real value 0.0, then for n = 0, 1, ..., N-1 in increasing order add a[n] times b[n]. Each product is rounded to f64 before it is added, and each addition is rounded. Do not use fused multiply-add and do not reorder the additions. Where a formula says "in index order" the same rule applies. The encoder output is compared with the reference bit for bit in at least 95 percent of the frame groups, so the order of operations matters wherever a search compares two nearly equal scores. Follow the stated order everywhere, in particular in the recursions of sections 4, 7 and 9.
4. A filter written as a sum over past values is evaluated as the dot product of the coefficient vector with a delay line. Coefficient number j (counted from 1) multiplies the value j samples ago. The delay line has the most recent value in entry 0, so entry 0 is paired with coefficient number 1. The delay line is shifted after the dot products for the sample have been formed.
5. LSP frequencies are expressed in units of pi/256 radian (15.625 Hz). A frequency w in these units corresponds to the angular frequency pi w / 256 and to the position w in `LSP_COSINE_TABLE`. All LSP constants in this document (3.0, 252.0, 2.0, 4.0, 1/32) are in these units.
6. Integer division of non-negative integers truncates. Conversions of a positive real number to an integer truncate toward zero.
7. The 16-bit sine-detector register of section 4.5 uses wrap-around arithmetic; it is the only place where a 16-bit overflow matters.
8. Tables are referenced by their names in `tables.md`. Where a table holds a value that the reference prints with a known deviation (for example entry 6 of `WEIGHTING_FILTER_ZERO_FACTORS`), the table value is used.

## 2. Encoder state and initialization (clause 2.21)

The encoder owns the following persistent state. Nothing else survives from one frame to the next when VAD and comfort noise are disabled. (The reference also has voice activity detector and comfort noise state. Nothing in it is read by code that influences the bitstream when the VAD is disabled, so it is not part of the encoder state.)

| State | Length | Initial value | Updated in |
|---|---|---|---|
| High-pass filter input memory | 1 | 0 | 3.1 |
| High-pass filter output memory | 1 | 0 | 3.1 |
| Input history (the last 120 high-pass filtered samples of the previous frame) | 120 | all 0 | 3, step 6 |
| Previous quantized LSP vector | 10 | `LSP_DC_VECTOR` | 3, step 11 |
| Weighted-speech history | 145 | all 0 | 3, step 10 |
| Frame-level weighting filter FIR memory | 10 | all 0 | 6.2 |
| Frame-level weighting filter IIR memory | 10 | all 0 | 6.2 |
| Ringing history | 145 | all 0 | 9.6 |
| Subframe-level synthesis and weighting FIR memory | 10 | all 0 | 9.6 |
| Subframe-level weighting IIR memory | 10 | all 0 | 9.6 |
| Excitation history | 145 | all 0 | 9.5 |
| Sine-detector register | 16 bits | 0 | 4.5 |
| Taming error vector | 5 | each entry 0.00000381464 | 9.4.3 |

The constant 0.00000381464 is called ERR0 below. It is a decimal literal; use exactly this number (it is close to 2 to the power -18 but not equal to it).

The text of clause 2.21 says that all static variables start at zero except the previous LSP vector. The reference also initializes the five taming error entries to ERR0. The text does not say this.

A full encoder reset (what the reference does when the conformance runs reinitialize every 10 frames) restores every row of the table to its initial value. The reset happens after a frame has been completely encoded and before the next frame is processed. It is independent of the decoder reset. The coder rate is not part of the state.

## 3. Frame flow

The encoder is called once per frame with 240 input samples (clause 2.2). The reference driver drops a trailing partial frame of the input file, so the encoder only ever sees full frames.

Notation. s[n], n = 0 to 239, is the input frame and x[n] is the high-pass filtered frame. B is the array of 360 samples formed by the 120-sample input history followed by x[0..239]. The "frame buffer" is a 240-sample array. After step 6 below it holds c[n] = B[60 + n]. The coded frame c is the high-pass filtered signal delayed by 60 samples (7.5 ms), which is the look-ahead of clause 1.4. Subframe k (k = 0 to 3) of the coded frame is c[60 k .. 60 k + 59]. From step 8 on, the frame buffer is overwritten in place, first by the formant-weighted speech, then by the harmonic-noise-shaped target, then (inside the subframe loop) by the target, the residual and the fixed excitation of each subframe.

Order of operations for an active frame (the only kind when VAD is disabled):

1. High-pass filter the input frame (section 3.1). The result is x.
2. LPC analysis (section 4): form B and compute four unquantized LPC sets, one per subframe, from windows that start at B[60 k], k = 0 to 3. This also updates the sine-detector register.
3. Convert the LPC set of subframe 3 to an LSP vector (section 5.1), with the previous quantized LSP vector as fallback.
4. The voice activity decision is skipped: the frame is active.
5. Quantize the LSP vector (section 5.2), obtaining the 24-bit LSP index. The quantizer overwrites the unquantized LSP vector with its prediction residual. The unquantized LSP vector is not used again.
6. Shift the input history: the new input history is B[240..359]. The frame buffer becomes c[n] = B[60 + n], n = 0 to 239.
7. Compute the perceptual weighting filter coefficients of the four subframes from the unquantized LPC sets (section 6.1).
8. Filter the frame buffer with the perceptual weighting filter, subframe by subframe, in place (section 6.2). The frame buffer is now the formant-weighted speech f[n], n = 0 to 239.
9. Form the analysis buffer g of 385 samples: the 145 samples of the weighted-speech history followed by f[0..239]. Compute two open-loop pitch estimates (section 7): the first with Start = 145, the second with Start = 265.
10. Compute the harmonic noise shaping parameters of the four subframes (section 8.1), all from g. Then set the weighted-speech history to g[240..384], that is f[95..239]. Then replace the frame buffer by the harmonic-noise-shaped target w[n] (section 8.2). The weighted-speech history receives f, not w.
11. Inverse quantize the LSP index (section 5.3), interpolate and convert to four quantized LPC sets (section 5.4), and store the quantized LSP vector as the new previous quantized LSP vector.
12. For each subframe k = 0, 1, 2, 3 in order, perform the subframe processing of section 9.
13. Pack the parameters (bitstream document).

At the low rate the reference driver also calls a reset of the ACELP search work counter before every frame (see the fixed codebook document). It has no other effect on the analysis path.

### 3.1 High-pass filter (clause 2.3)

The filter of equation 1 is H(z) = (1 - z^-1) / (1 - (127/128) z^-1). The reference runs it with two memories, one for the previous input (xin) and one for the previous output (yout), both initially 0. For n = 0 to 239 in order:

1. d = s[n] - xin.
2. xin = s[n].
3. yout = d + yout times 0.9921875 (this is 127.0 / 128.0 exactly).
4. x[n] = yout.

The memories persist across frames. The filter is enabled by default in the reference driver. (A command line option disables it; then x = s and the memories stay 0. The conformance runs use the default.)

## 4. LPC analysis (clause 2.4)

### 4.1 Window placement

For subframe k = 0 to 3 form the 180-sample windowed vector v_k[n] = B[60 k + n] times `HAMMING_WINDOW`[n], n = 0 to 179. Subframe k of the coded frame covers B[60 k + 60 .. 60 k + 119], so the window is centred on it. Subframe 3 uses B[180..359], the last 180 samples of the current input frame.

### 4.2 Autocorrelation and corrections

For each subframe, with each operation performed in the order written:

1. R[0] = (dot product of length 180 of v_k with itself) / 32400.0.
2. R[0] = R[0] times 1.0009765625 (this is 1025.0 / 1024.0 exactly). This is the white noise correction of the text.
3. If R[0] is exactly 0.0, set R[1] to R[10] to 0.0.
4. Otherwise, for j = 1 to 10: R[j] = ((dot product of length 180 - j of v_k[0..] with v_k[j..]) / 32400.0) times `BINOMIAL_WINDOW`[j - 1].

The divisor 32400 is 180 times 180. The binomial multiplication comes after the division.

### 4.3 Levinson-Durbin (procedure Durbin)

Inputs: R[1..10] and the energy E, which starts as R[0]. Outputs: the LPC coefficients a[1..10] (predictor sign convention: the synthesis filter is 1 / (1 - sum of a[j] z^-j), equation 2) and a value PK2 used by the sine detector.

1. Set all ten coefficients to 0.0.
2. For order i = 1 to 10, in this order:
   1. t = R[i]. Then for j = 1 to i - 1 in increasing order subtract a[j] times R[i - j] from t, one product at a time.
   2. Guard: if the absolute value of t is greater than or equal to E (the comparison includes equality), set PK2 = 0.99 and end the recursion at once. The coefficients keep their present values: a[1..i-1] hold the solution of order i - 1 and a[i..10] are 0.0. The guard also triggers when E is zero or negative; in particular when R[0] was exactly 0.0, every coefficient stays 0.0 and PK2 is 0.99.
   3. k = t / E. Set a[i] = k.
   4. E = E - t times k.
   5. If i = 2, set PK2 = -k (the second reflection coefficient with its sign changed).
   6. For j = 1 to i - 1: a[j] = a[j] - k times a_old[i - j], where a_old[1..i-1] is a copy of a[1..i-1] taken before this update. Coefficient a[i] is not changed by this loop.

PK2 is therefore the value assigned at order 2, unless the guard triggers at order 3 or higher, in which case the guard overwrites it with 0.99, or the guard triggers at order 1 or 2, in which case it is 0.99.

The text does not describe the guard or PK2.

### 4.4 Output

The four LPC sets a_k[1..10], k = 0 to 3, are the unquantized LPC sets. They are used by the perceptual weighting filter (section 6) and, for k = 3, by the LSP conversion (section 5.1). The reference also sums the four autocorrelation vectors of the frame for the comfort noise generator; that sum is not used when VAD is disabled.

### 4.5 Sine detector

The reference keeps a 16-bit register, initially 0. After the Levinson-Durbin recursion of each subframe k (k = 0 to 3, in order) the register is updated:

1. Shift the register left by one bit, discarding the top bit (16-bit wrap).
2. If PK2 > 0.95 (strict), add 1 to the register.

After the fourth subframe of the frame:

3. Clear bit 15 (AND with 0x7FFF). The low 15 bits now hold the decisions of the last 15 subframes, bit 0 being the newest.
4. Count the 1 bits among bits 0 to 14.
5. If the count is 14 or more, set bit 15.

The users of the register treat it as a signed 16-bit integer: it is "negative" exactly when bit 15 is set. Bit 15 is rebuilt every frame (step 3 clears the old value before step 5), so the flag depends only on the last 15 subframe decisions. The flag is read by the voice activity detector of Annex A and by the taming procedure of section 9.4.2, where it forces the most restrictive limit on the gain codebook search. The second use is part of the encoder even when VAD is disabled.

## 5. LSP conversion, quantization and interpolation (clauses 2.5, 2.6, 2.7, 3.10.1)

### 5.1 LPC to LSP conversion (procedure AtoLsp)

Input: the unquantized LPC set a[1..10] of subframe 3 and the previous quantized LSP vector. Output: ten LSP frequencies in increasing order, in units of pi/256.

1. Bandwidth expansion (7.5 Hz, clause 2.5): v[j] = a[j] times `BANDWIDTH_EXPANSION_FACTORS`[j - 1], j = 1 to 10.
2. Build the twelve-entry sequence G[0..11]. Set G[0] = 1.0 and G[1] = 1.0. For m = 0 to 4 in order:
   - G[2 m + 2] = (-G[2 m] - v[m + 1]) - v[10 - m]
   - G[2 m + 3] = (G[2 m + 1] - v[m + 1]) + v[10 - m]
   
   Then multiply G[10] and G[11] by 0.5. The even-indexed entries define the sum polynomial and the odd-indexed entries the difference polynomial, each with its real root already divided out.
3. For an integer grid index q define two functions:
   - Pe(q) = the sum over j = 0 to 5 of G[10 - 2 j] times `LSP_COSINE_TABLE`[(q j) mod 512], accumulated in increasing j starting from 0.0.
   - Po(q) = the sum over j = 0 to 5 of G[11 - 2 j] times `LSP_COSINE_TABLE`[(q j) mod 512], accumulated in the same way.
4. Root search. The active function is Pe at the start. Let prev = Pe(0) and let the list of roots be empty. For q = 1 to 255 in increasing order:
   1. cur = the active function evaluated at q.
   2. If cur times prev < 0 (the product is strictly negative; a product of exactly zero is not a crossing):
      1. Let ap be the absolute value of prev and ac the absolute value of cur. Append the root (q - 1) + ap / (ap + ac) to the list.
      2. If the list has 10 entries, end the search.
      3. Switch the active function (Pe to Po, or Po to Pe).
      4. Evaluate cur again with the new active function at the same q.
   3. prev = cur.
5. If the search ended with fewer than 10 roots, discard them all and use the previous quantized LSP vector (all ten entries) as the result. The reference does this silently. Otherwise the list is the result.

### 5.2 LSP quantization (procedures Lsp_Qnt and Lsp_Svq, clause 2.5)

Input: the unquantized LSP vector p'[1..10] of the current frame and the previous quantized LSP vector q_prev[1..10]. Output: the 24-bit index.

1. Weights (equation 5), from the unquantized vector p':
   - W[1] = 1.0 / (p'[2] - p'[1]). There is no guard against a zero difference.
   - W[10] = 1.0 / (p'[10] - p'[9]). No guard.
   - For j = 2 to 9: let m = p'[j + 1] - p'[j] and let m2 = p'[j] - p'[j - 1]. If m2 < m, then m = m2. If m > 0.0, W[j] = 1.0 / m, otherwise W[j] = 1.0.
2. Prediction. For j = 1 to 10: e[j] = (p'[j] - `LSP_DC_VECTOR`[j - 1]) - 0.375 times (q_prev[j] - `LSP_DC_VECTOR`[j - 1]). The factor 0.375 = 12/32 is the predictor b of equation 3.3. The vector e is the search target; the unquantized p' is not used again.
3. For each band m = 0, 1, 2 (sub-vectors of 3, 3 and 4 entries, starting at entries 1, 4 and 7, as in `LSP_SUBVECTOR_LAYOUT`) search the 256 entries of the codebook of the band (`LSP_BAND0_CODEBOOK`, `LSP_BAND1_CODEBOOK`, `LSP_BAND2_CODEBOOK`), index l = 0 to 255 in increasing order:
   1. For each coordinate d of the band, tw[d] = W[start + d] times cb[l][d], where start is the first entry number of the band.
   2. score(l) = 2.0 times (dot product of the target sub-vector e[start..] with tw) minus (dot product of cb[l] with tw). Both dot products follow the rule of section 1.
   3. The best score is initialized to -1.0 and the best index to 0. An entry replaces the best only if its score is strictly greater than the best score.
   
   Maximizing the score is the same as minimizing the weighted squared error of equation 4.5. Entry 0 of every band codebook is all zeros, so its score is exactly 0.0 and the initial values never matter; on ties the lowest index wins.
4. The result is band 0 index times 65536, plus band 1 index times 256, plus band 2 index.

The text numbers the codebook entries 1 to 256; the transmitted index is 0 to 255.

### 5.3 LSP inverse quantization and stability check (procedure Lsp_Inq, clauses 2.6 and 3.10.1)

Inputs: the 24-bit index, the previous quantized LSP vector q_prev and an erasure flag. The encoder always passes "not erased". The decoder document refers to this section for both variants.

1. Select the variant:
   - Normal frame: minimum separation S = 2.0 (31.25 Hz) and predictor factor P = 0.375 (12/32).
   - Erased frame (clause 3.10.1): the index is replaced by 0, S = 4.0 (62.5 Hz) and P = 0.71875 (23/32).
2. Split the index: band 2 is the low 8 bits, band 1 the next 8 bits, band 0 the highest 8 bits. Copy the codebook entries into the vector e: e[1..3] from `LSP_BAND0_CODEBOOK`, e[4..6] from `LSP_BAND1_CODEBOOK` and e[7..10] from `LSP_BAND2_CODEBOOK`. For an erased frame all three indices are 0, and entry 0 of each of the three codebooks is all zeros, so e is the zero vector, as clause 3.10.1 says.
3. For j = 1 to 10: p[j] = (e[j] + (q_prev[j] - `LSP_DC_VECTOR`[j - 1]) times P) + `LSP_DC_VECTOR`[j - 1].
4. Stability loop. Run at most 10 iterations. In each iteration:
   1. If p[1] < 3.0, set p[1] = 3.0.
   2. If p[10] > 252.0, set p[10] = 252.0.
   3. For j = 2 to 10 in increasing order: d = (S + p[j - 1]) - p[j]. If d > 0.0, then d = d times 0.5, p[j - 1] = p[j - 1] - d and p[j] = p[j] + d. The pass is sequential: the update for j uses the p[j - 1] already changed by the update for j - 1.
   4. Test: the vector is "unstable" if some j from 2 to 10 has p[j] - p[j - 1] < S - 0.03125. If the vector is not unstable, leave the loop.
5. If the loop ended after 10 iterations with the vector still unstable, set p to q_prev (all ten entries).
6. The result is p.

The reference differs from the text (equations 6 and 7.1 to 7.3) in three ways. It tests against S - 1/32 (31.25 Hz minus 0.5 Hz) instead of S. It corrects every adjacent pair that is closer than S, in a sequential pass. And it clamps p[1] to at least 3.0 and p[10] to at most 252.0 at the start of every iteration. The correction of step 4.3 keeps the mean of the pair and moves the two values to a distance of exactly S, as in equations 7.1 to 7.3.

### 5.4 Interpolation and conversion to LPC (procedures Lsp_Int and LsptoA, clause 2.7)

Input: the quantized LSP vector p of the current frame and the previous quantized LSP vector q_prev. Output: four quantized LPC sets ã_k[1..10], k = 0 to 3.

1. For k = 0 to 3 and j = 1 to 10: u_k[j] = (1.0 - `LSP_INTERPOLATION_WEIGHTS`[k]) times q_prev[j] + `LSP_INTERPOLATION_WEIGHTS`[k] times p[j] (equation 8). For k = 3 the first term is exactly 0.0.
2. Convert u_k to ã_k with the procedure below.

LSP to LPC conversion of a vector u of ten frequencies, each between 0 and 255:

1. For j = 1 to 10: let fl = floor(u[j]) as an integer; x[j] = -(`LSP_COSINE_TABLE`[fl] + (`LSP_COSINE_TABLE`[fl + 1] - `LSP_COSINE_TABLE`[fl]) times (u[j] - fl)). This is the negated cosine of the frequency, by linear interpolation in the table.
2. The sum polynomial uses x[1], x[3], x[5], x[7], x[9]; the difference polynomial uses x[2], x[4], x[6], x[8], x[10]. Build two arrays P[0..5] and Q[0..5]:
   - P[0] = 0.5, P[1] = x[1] + x[3], P[2] = 1.0 + 2.0 times x[1] times x[3].
   - Q[0] = 0.5, Q[1] = x[2] + x[4], Q[2] = 1.0 + 2.0 times x[2] times x[4].
   - For r = 2, 3, 4 in order let xp = x[2 r + 1], let xq = x[2 r + 2] and let F = `LSP_POLYNOMIAL_SCALES`[r - 2] (the values are 1.0, 0.5, 0.25 for r = 2, 3, 4). Update P in four steps:
     1. P[r + 1] = P[r - 1] + P[r] times xp.
     2. For j = r down to 2 (decreasing order): P[j] = P[j - 1] times xp + 0.5 times (P[j] + P[j - 2]).
     3. P[0] = P[0] times 0.5.
     4. P[1] = (P[1] + xp times F) times 0.5.
     
     Update Q in the same four steps with xq in place of xp and the same F. Because j decreases in step 2, the values P[j - 1] and P[j - 2] on the right side are still the values from before this round.
3. For i = 0 to 4:
   - ã[i + 1] = (((-P[i] - P[i + 1]) + Q[i]) - Q[i + 1]) times 8.0
   - ã[10 - i] = (((-P[i] - P[i + 1]) - Q[i]) + Q[i + 1]) times 8.0

The result is the predictor-form LPC set (synthesis filter 1 / (1 - sum of ã[j] z^-j), equation 10) with no further scale.

Role of `LSP_POLYNOMIAL_SCALES`: each round multiplies a polynomial by the quadratic factor of one more frequency and keeps the stored coefficients at half of their true size. The true update of the first stored coefficient needs twice the leading coefficient; the leading coefficient is 0.5, 0.25, 0.125 at the start of rounds 2, 3, 4 (it is halved in every round), so twice its value is 1.0, 0.5, 0.25, which are the table entries. The text does not give these values.

This conversion was checked numerically against the forward conversion of section 5.1: converting a stable LPC set to LSP and back reproduces the bandwidth-expanded LPC set to within about 5e-4 in every coefficient.

The encoder uses the quantized sets ã_k only for the synthesis part of the filters of sections 9.1, 9.2 and 9.6. The weighting filters keep using the unquantized sets.

## 6. Formant perceptual weighting (clause 2.8)

### 6.1 Coefficients (procedure Wght_Lpc)

For each subframe k = 0 to 3 and j = 1 to 10 compute from the unquantized LPC set a_k[1..10]:
- numerator coefficients bnum_k[j] = a_k[j] times `WEIGHTING_FILTER_ZERO_FACTORS`[j - 1] (0.9 to the power j; entry 6 of the table is the printed 0.4781969, not the exact power),
- denominator coefficients bden_k[j] = a_k[j] times `WEIGHTING_FILTER_POLE_FACTORS`[j - 1] (0.5 to the power j).

### 6.2 Filtering (procedure Error_Wght)

The weighting filter of equation 11 is applied to the frame buffer in place, with the coefficients of subframe k applied to the 60 samples of subframe k. It uses two persistent frame-level memories (the last ten inputs and the last ten outputs, entry 0 the most recent). They are neither reset nor converted at subframe boundaries; only the coefficients change. For each sample c in order:

1. y = c - (dot product of length 10 of bnum_k[1..10] with the FIR memory).
2. Shift the FIR memory by one place and put c in entry 0.
3. y = y + (dot product of length 10 of bden_k[1..10] with the IIR memory).
4. Shift the IIR memory by one place and put y in entry 0.
5. The output sample is y.

Both dot products use the memories as they were before this sample's shift. The outputs form f[n], n = 0 to 239.

## 7. Open-loop pitch estimation (clause 2.9, procedure Estim_Pitch)

The analysis buffer g has 385 entries: g[0..144] is the weighted-speech history and g[145 + n] = f[n], n = 0 to 239. The estimator runs twice. The first run has Start = 145 and gives the open-loop lag LOL(0) of subframes 0 and 1. The second run has Start = 265 and gives LOL(1) of subframes 2 and 3.

One run, for a given Start:

1. Initialize the best lag Lbest = 18, the best energy Ebest = 1.0 and the best squared correlation Cbest = 0.0.
2. E = dot product of length 120 of g[Start - 17 ..] with itself. This is the energy of the segment for lag 17, one lag below the first candidate.
3. For each candidate lag j = 18 to 142 in increasing order:
   1. Update the running energy: E = (E - a) + b, with a = g[Start - j + 120] times g[Start - j + 120] and b = g[Start - j] times g[Start - j]. E is now the energy of g[Start - j .. Start - j + 119]. The energy is updated recursively and never recomputed.
   2. C = dot product of length 120 of g[Start ..] with g[Start - j ..]. C2 = C times C.
   3. D = C2 times Ebest - E times Cbest.
   4. If E > 0.0 and C > 0.0 (both strict) and either of the following holds, set Lbest = j, Ebest = E and Cbest = C2:
      - D > 0.0 and j - Lbest < 18, or
      - D > 0.25 times C2 times Ebest.
4. The result is Lbest, between 18 and 142.

Meaning. The criterion is C2 / E (equation 12, positive correlation only). Since Ebest starts at 1.0 and Cbest at 0.0, the first candidate with E > 0 and C > 0 is accepted whatever its lag. After that, a candidate within 17 samples above the current best replaces it if its criterion is strictly larger. A candidate 18 or more samples above the best replaces it only if the second condition holds, which is equivalent (for positive E and Ebest) to C2 / E being larger than (4/3) times Cbest / Ebest. The factor 4/3 is 1.249 dB. The text says "1.25 dB"; the reference implements the factor as the second condition above and does not compute a decibel value. The second condition is also checked for candidates close to the best; it is simply or-ed with the first.

## 8. Harmonic noise shaping (clause 2.11)

### 8.1 Parameters (procedure Comp_Pw)

For each subframe k = 0 to 3 let S = 145 + 60 k and let L0 = LOL(k div 2), the open-loop lag. The closed-loop search has not yet changed it: all four subframes are computed before the subframe loop starts. The signal is g of section 7, unmodified.

1. Et = dot product of length 60 of g[S ..] with itself.
2. Initialize: no candidate accepted, Eb = 1.0, Cb = 0.0 and C2b = 0.0.
3. For i = 0 to 6 in increasing order, with candidate lag j = L0 - 3 + i:
   1. C = dot product of length 60 of g[S ..] with g[S - j ..].
   2. E = dot product of length 60 of g[S - j ..] with itself.
   3. If E > 0.0 and C > 0.0: C2 = C times C. If C2 times Eb > E times C2b (strict), accept this candidate: the accepted index is i, Eb = E, Cb = C and C2b = C2.
4. If no candidate was accepted, the lag is L0 and the gain is 0.0.
5. Otherwise the lag is L0 - 3 + (accepted index). The gain is 0.0, except that if C2b > (Eb times Et) times 0.375 (strict) then Gopt = 1.0 if Cb > Eb, otherwise Gopt = Cb / Eb, and the gain is 0.3125 times Gopt. (The reference also sets Gopt = 1.0 when Eb is zero; Eb is positive here, so this case does not occur.)

The text (equations 14 to 17) states the activation as a 2.0 dB test, -10 log10(1 - CL / E) >= 2.0. The reference uses the fixed threshold 0.375 on C2b / (Eb Et), as written in step 5. Use the reference. The lag is between 15 and 145.

### 8.2 Applying the filter (procedure Filt_Pw)

For subframe k with lag L_k and gain beta_k, the target is w[60 k + n] = g[145 + 60 k + n] - beta_k times g[145 + 60 k + n - L_k], n = 0 to 59. It is computed from g (the formant-weighted speech with its history), never from earlier outputs w. When L_k is less than 60, the second term reads formant-weighted samples of the same subframe. The values w replace the frame buffer.

The lags L_k and gains beta_k are kept for the subframe loop.

## 9. Subframe processing (clauses 2.12 to 2.14, 2.18, 2.19 and taming)

For subframe k (0 to 3) the inputs are: the quantized LPC set ã_k, the coefficient sets bnum_k and bden_k of section 6.1, the shaping lag L_k and gain beta_k of section 8, and the target w (frame buffer positions 60 k .. 60 k + 59). In order:

1. Impulse response (9.1).
2. Ringing subtraction (9.2), giving the target t.
3. Pitch predictor search (9.3, limited by the taming procedure of 9.4), giving the closed-loop lag, the lag offset index, the pitch gain index and the residual r.
4. Fixed codebook search (section 10).
5. Excitation reconstruction and excitation history update (9.5).
6. Taming error update (9.4.3).
7. Filter memory update (9.6).

### 9.1 Impulse response (clause 2.12, procedure Comp_Ir)

The combined filter is S(z) = A~(z) W(z) P(z) of equation 19. Its impulse response h[n], n = 0 to 59, is computed with all memories zero. Keep a synthesis delay line Y and a weighting delay line Z (10 entries each, all 0.0) and an array T of 205 entries, all 0.0. The input is u[0] = 1.0 and u[n] = 0.0 for n = 1 to 59. For n = 0 to 59:

1. ys = u[n] + (dot product of length 10 of ã_k[1..10] with Y).
2. q = ys - (dot product of length 10 of bnum_k[1..10] with Y).
3. Shift Y by one place and put ys in entry 0.
4. q = q + (dot product of length 10 of bden_k[1..10] with Z).
5. Shift Z by one place and put q in entry 0.
6. T[145 + n] = q.
7. h[n] = q - beta_k times T[145 - L_k + n].

Step 2 uses Y before the shift of step 3 and step 4 uses Z before the shift of step 5. T[145 - L_k + n] is 0.0 or an earlier q of this loop (L_k is at least 15). The value h[0] is exactly 1.0, which section 9.3.3 uses.

### 9.2 Zero input response and ringing subtraction (clause 2.13, procedure Sub_Ring)

1. Copy the persistent subframe-level memories into Y (from the synthesis and weighting FIR memory) and Z (from the weighting IIR memory), and the 145 entries of the ringing history into T[0..144], with T[145..204] = 0.0. The persistent memories are not changed by this step.
2. For n = 0 to 59:
   1. ys = dot product of length 10 of ã_k[1..10] with Y. There is no input term.
   2. q = ys - (dot product of length 10 of bnum_k[1..10] with Y).
   3. Shift Y by one place and put ys in entry 0.
   4. q = q + (dot product of length 10 of bden_k[1..10] with Z).
   5. Shift Z by one place and put q in entry 0.
   6. T[145 + n] = q.
   7. z = q - beta_k times T[145 - L_k + n].
   8. t[n] = w[n] - z.
3. The vector t replaces w in the frame buffer.

For the first L_k samples, T[145 - L_k + n] reads the ringing history, that is, the filter output of the previous subframes for the actual excitation, so the shaping filter memory spans subframe boundaries.

### 9.3 Pitch predictor (clause 2.14; procedures Find_Acbk, Get_Rez, Decod_Acbk; clause 2.18)

The pitch predictor has 5 taps. The excitation history EH has 145 entries; entry 144 is the newest. Write e[-m] = EH[145 - m] for m = 1 to 145, the excitation m samples before the first sample of the present subframe.

#### 9.3.1 Residual extraction (procedure Get_Rez, equation 41.1)

For a lag Lg between 17 and 143 build the 64-entry array RZ:
- RZ[0] = e[-Lg - 2] and RZ[1] = e[-Lg - 1]
- RZ[2 + i] = e[(i mod Lg) - Lg], that is EH[145 - Lg + (i mod Lg)], for i = 0 to 61.

The text writes e'[n] = e[(n mod Li) - Li] for 2 <= n <= 63. The reference differs: the periodic repeat is indexed from n - 2, so the entry at position n is e[((n - 2) mod Li) - Li]. This is what makes RZ[n + 2] the excitation delayed by exactly Lg samples, repeated periodically when Lg is less than 62. Use the reference form.

#### 9.3.2 Adaptive codebook contribution (equation 41.2, procedure Decod_Acbk)

Inputs: the pair lag Lp, the lag offset index LO and the pitch gain index GI.

1. Form RZ with the lag Lp + LO - 1.
2. Select the codebook CB: at the low rate `PITCH_GAIN_CODEBOOK_170`; at the high rate `PITCH_GAIN_CODEBOOK_170` if Lp >= 58, otherwise `PITCH_GAIN_CODEBOOK_85`.
3. p[n] = dot product of length 5 of RZ[n .. n + 4] with columns 0 to 4 of row GI of CB, for n = 0 to 59.

For even subframes Lp is the new closed-loop lag and LO is 1, so the lag used is Lp. For odd subframes Lp is the closed-loop lag of the preceding even subframe and LO is the transmitted 2-bit value, so the lag used is Lp - 1 + LO. The decoder uses the same procedure (clauses 3.4 and 2.18).

#### 9.3.3 Closed-loop search (procedure Find_Acbk)

Inputs: the target t, the impulse response h, the excitation history, the pair lag, the coder rate. Write LB for the lag base.

1. Lag base and candidates.
   - Even subframes (k = 0, 2): LB = LOL(k div 2). If LB = 18 then LB = 19. If LB > 140 then LB = 140. The number of candidates is H = 3.
   - Odd subframes (k = 1, 3): LB = the pair lag Lp stored by the preceding even subframe (step 5 below). No clamping. H = 4.
   - Candidate d = 0 to H - 1 has the lag LB - 1 + d. Even subframes search LB - 1, LB, LB + 1 (lags 18 to 141 over all cases); odd subframes search LB - 1, LB, LB + 1, LB + 2 (lags 17 to 143; lag 17 occurs when the even subframe chose 18).
2. For each candidate d, with lag Lg = LB - 1 + d:
   1. Form RZ for Lg (9.3.1).
   2. Form the five filtered vectors Fv[0..4], each of 60 entries, by these recurrences (follow them as written rather than convolving independently):
      - Fv[4][n] = the sum over j = 0 to n of RZ[4 + j] times h[n - j], in increasing j, starting from 0.0.
      - For m = 3 down to 0: Fv[m][0] = RZ[m] (this equals RZ[m] times h[0] because h[0] = 1.0), and for n = 1 to 59: Fv[m][n] = RZ[m] times h[n] + Fv[m + 1][n - 1].
   3. Build the 20 correlation terms CV[0..19]:
      - CV[m] = dot product of length 60 of t with Fv[m], m = 0 to 4.
      - CV[5 + m] = 0.5 times (dot product of length 60 of Fv[m] with itself), m = 0 to 4.
      - CV[10..19]: for m = 1 to 4 (outer loop) and q = 0 to m - 1 (inner loop) in turn, the next entry is the dot product of length 60 of Fv[m] with Fv[q]. The order of the pairs is (1,0), (2,0), (2,1), (3,0), (3,1), (3,2), (4,0), (4,1), (4,2), (4,3). This is the pair order of columns 10 to 19 of the pitch gain codebooks, (0,1), (0,2), (1,2), (0,3), (1,3), (2,3), (0,4), (1,4), (2,4), (3,4).
3. Search limits from taming. Let Lag1 = LB - 1 and Lag2 = LB - 1 + H - 1. The taming test of 9.4.2 returns the limit level TL. Then Bound85 = min(85, 51 + 4 TL) and Bound170 = min(170, 93 + 8 TL).
4. Search. Initialize the best score to 0.0, the gain index GI to 0 and the lag offset index LO to 1. For d = 0 to H - 1 in increasing order (outer loop) and for gain row i = 0 upward below the bound (inner loop):
   1. The codebook and bound for candidate d:
      - Low rate: `PITCH_GAIN_CODEBOOK_170` with Bound170.
      - High rate, even subframe: `PITCH_GAIN_CODEBOOK_170` with Bound170 if LB - 1 + d >= 58, otherwise `PITCH_GAIN_CODEBOOK_85` with Bound85. The decision uses the candidate's own lag.
      - High rate, odd subframe: `PITCH_GAIN_CODEBOOK_170` with Bound170 if LB >= 58, otherwise `PITCH_GAIN_CODEBOOK_85` with Bound85. The decision uses LB for all four candidates.
   2. score = dot product of length 20 of CV with row i of the codebook (columns 0 to 19).
   3. If score > best score (strict), set best score = score, GI = i and LO = d.
   
   Ties go to the earliest candidate in this order. If no score is strictly positive the result is GI = 0 and LO = 1.
   
   The score is the correlation of the target with the predicted vector minus half the energy of the predicted vector: columns 0 to 4 of a row are the tap gains b, columns 5 to 9 the negated squares and columns 10 to 19 the negated products of tap pairs, so the score equals the sum of b[m] CV[m] minus 0.5 times the squared norm of the sum of b[m] Fv[m]. Maximizing it minimizes the error between the target and the filtered prediction.
5. Lag coding.
   - Even subframes: the closed-loop lag is LB - 1 + LO. The pair lag Lp is set to this lag (it replaces the open-loop lag). The stored lag offset index is set to 1; it is not transmitted. The transmitted 7-bit value is Lp - 18, between 0 and 123.
   - Odd subframes: Lp is unchanged. The offset index LO (0 to 3) is the transmitted 2-bit value; the lag used is Lp - 1 + LO, so it differs from Lp by -1, 0, +1 or +2.
   - The pitch gain index GI is stored for the subframe.
6. Residual. Compute the adaptive contribution p with 9.3.2 using Lp, LO and GI. Then for n = 0 to 59: r[n] = t[n] minus, one product at a time in increasing j, the products p[j] times h[n - j] for j = 0 to n. The residual r replaces t in the frame buffer (equation 20 with the contribution filtered by h).

#### 9.3.4 Differences between text and reference

The text says the even-subframe lag is searched within plus or minus 1 of the open-loop lag and the odd-subframe offset takes the values -1, 0, +1, +2. The text does not say that the open-loop lag is first moved into the range 19 to 140, that a lag as low as 17 or as high as 143 can occur in odd subframes, that the search over the gain codebook is limited by the taming procedure, or that the choice of gain codebook in odd subframes uses the lag of the preceding even subframe for all candidates.

### 9.4 Taming (no counterpart in the text)

The reference keeps a vector Err of five estimates, each for a stretch of 30 samples of the excitation history, and uses it to limit how many rows of the pitch gain codebooks the search of 9.3.3 may use. The reference does not document its purpose beyond a comment that it avoids excitation growth when the encoder and decoder are out of step. As an orientation only: entry z of Err stands for the excitation between 30 z + 1 and 30 z + 30 samples before the start of the present subframe, with entry 0 the most recent, and each entry is a worst-case amplification estimate that grows when the pitch predictor uses large gains. The exact rules follow.

#### 9.4.1 Tables

Let beta(GI) be the entry GI of `TAMING_GAIN_BOUNDS_85` when the 85-row gain codebook was used and of `TAMING_GAIN_BOUNDS_170` when the 170-row codebook was used. Both tables are in increasing order, so a limit on the row index limits the gains.

#### 9.4.2 Limit level (taming test)

Called at step 3 of 9.3.3, before the correlation search, with Lag1 and Lag2 as defined there.

1. i2 = Lag2 + 2, zone2 = i2 div 30.
2. i1 = Lag1 - 61. If i1 <= 0, set i1 = 1. zone1 = i1 div 30.
3. Emax = -1.0. For z = zone2 down to zone1 (inclusive): if Err[z] > Emax then Emax = Err[z]. (zone2 is at most 4 because Lag2 is at most 143.)
4. If Emax > 128.0 (strict), or the sine-detector register is negative (bit 15 set, section 4.5), TL = 0.
5. Otherwise TL = the integer part of (128.0 - Emax), truncated. TL is between 0 and 127.

With TL = 0 only the first 51 rows of the 85-row codebook and the first 93 rows of the 170-row codebook can be selected. From TL = 9 on, all 85 rows are available (51 + 4 times 9 = 87 is capped at 85), and from TL = 10 on all 170 rows (93 + 8 times 10 = 173 is capped at 170).

#### 9.4.3 Update

Performed for every subframe after the excitation history has been updated (9.5) and before the memory update (9.6). Inputs: the pair lag Lp as left by the search for this subframe, the subframe's lag offset index LO and its pitch gain index GI.

1. Lag = Lp - 1 + LO.
2. Select the table: `TAMING_GAIN_BOUNDS_85` if the coder rate is the high rate and Lp < 58, otherwise `TAMING_GAIN_BOUNDS_170` (always 170 at the low rate). beta is its entry GI.
3. If Lag <= 30: W0 = Err[0] times beta + ERR0, and W1 = W0.
4. Otherwise let iz = (Lag times 1092) shifted right by 15 bits, in integer arithmetic. For Lag from 31 to 150 this equals (Lag - 1) div 30: iz is 1 for lags 31 to 60, 2 for 61 to 90, 3 for 91 to 120 and 4 for 121 to 150. Let T2 = 30 times (iz + 1).
   - If T2 differs from Lag (Lag is not a multiple of 30):
     - If iz = 1: W0 = Err[0] times beta + ERR0 and W1 = Err[1] times beta + ERR0. Then set both W0 and W1 to the larger of the two.
     - If iz >= 2: Wt = Err[iz - 1] times beta + ERR0. W0 = Err[iz - 2] times beta + ERR0; if Wt > W0 then W0 = Wt. W1 = Err[iz] times beta + ERR0; if Wt > W1 then W1 = Wt.
   - If T2 equals Lag (Lag is 60, 90, 120 or 150): W0 = Err[iz - 1] times beta + ERR0 and W1 = Err[iz] times beta + ERR0.
5. If W0 > 256.0, set W0 = 256.0. If W1 > 256.0, set W1 = 256.0.
6. Shift: Err[4] = Err[2], then Err[3] = Err[1], then Err[2] = Err[0]. Then Err[0] = W0 and Err[1] = W1.

Each product Err times beta is rounded before ERR0 is added. Lag is at most 143, so iz is at most 4.

### 9.5 Excitation reconstruction and excitation history (clauses 2.17, 2.19)

After the fixed codebook search has replaced the residual in the frame buffer by the fixed excitation v[n] (n = 0 to 59):

1. Compute the adaptive contribution p[n] with 9.3.2, using Lp, LO and GI of this subframe (the same values as in 9.3.3 step 6).
2. The excitation is e[n] = v[n] + p[n].
3. Shift the excitation history: EH[m] = EH[m + 60] for m = 0 to 84 in increasing order. Then for n = 0 to 59: EH[85 + n] = -32768.0 if e[n] < -32767.5, else 32767.0 if e[n] > 32766.5, else e[n] unchanged. There is no rounding to an integer.
4. The unclipped e[n] is what the memory update of 9.6 uses. Only the history copy is clipped.

### 9.6 Memory update (clause 2.19, procedure Upd_Ring)

The subframe-level memories are updated by passing the unclipped excitation through the combined filter, with the coefficients of this subframe (ã_k, bnum_k, bden_k):

1. Shift the ringing history: RH[m] = RH[m + 60] for m = 0 to 84 in increasing order.
2. For n = 0 to 59, with the persistent subframe-level FIR memory Y and IIR memory Z:
   1. ys = e[n] + (dot product of length 10 of ã_k[1..10] with Y).
   2. q = ys - (dot product of length 10 of bnum_k[1..10] with Y).
   3. Shift Y by one place and put ys in entry 0.
   4. q = q + (dot product of length 10 of bden_k[1..10] with Z).
   5. Shift Z by one place and put q in entry 0.
   6. RH[85 + n] = q.

The harmonic filter P(z) has no memory of its own. Its memory is the ringing history RH, which stores the output q before the harmonic term, and the harmonic term is formed at the point of use (9.1 and 9.2).

## 10. Interface to the fixed codebook searches (clauses 2.15, 2.16)

The fixed codebook search is called at step 4 of each subframe, after the residual r has been formed.

Inputs: the residual r (60 samples, in the frame buffer); the impulse response h (60 samples) of 9.1; the coder rate; the subframe number k; the pair lag Lp as updated by 9.3.3 step 5; the lag offset index LO and the pitch gain index GI of this subframe.

At the high rate the search uses the pulse count of `MPMLQ_PULSE_COUNTS`[k]. It also tries the pulse-train variant when, and only when, Lp < 58; the pulse train period is Lp. At the low rate the search takes the pitch synchronous filter from `ACELP_PITCH_TAP_OFFSETS` and `ACELP_PITCH_TAP_GAINS` at the index GI, with the base lag Lp - 1 + LO (the lag used by the pitch predictor). At the low rate the search modifies h in place; h is not used after the search.

Outputs: the 60-sample fixed excitation vector v (including the quantized fixed gain and, at the high rate, the pulse train, and at the low rate the pitch synchronous contribution), which replaces the residual in the frame buffer; the fixed gain index FG (0 to 23, an index into `FIXED_GAIN_LEVELS`); the grid bit; the position index and the sign bits; and the pulse train bit (high rate only; 0 at the low rate).

## 11. The 12-bit GAIN parameter

For each subframe

GAIN = GI times 24 + FG, plus at the high rate only 2048 times the pulse train bit.

GI is the pitch gain row (0 to 84 for the 85-row codebook, 0 to 169 for the 170-row codebook), FG is the fixed gain index (0 to 23) and the pulse train bit is 0 or 1. At the low rate the pulse train bit is 0 and nothing is added. The pulse train bit can be 1 only at the high rate with Lp < 58; there the 85-row codebook is used, so GI times 24 + FG is at most 84 times 24 + 23 = 2039, which is below 2048 and does not collide with the pulse train bit. At the high rate with Lp >= 58 the pulse train bit is 0. The 12 bits are written least significant bit first by the bitstream packer.

The decoder inverts this: at the high rate with Lp < 58 the pulse train bit is bit 11 of the word and the other 11 bits are the value to split; in every other case the whole word is the value to split. The value is split as GI = value div 24 and FG = value mod 24. A GI at or above the size of the selected codebook is an invalid frame.

## 12. Parameters produced by this document

For each frame: the 24-bit LSP index (5.2); for each pair of subframes the transmitted 7-bit lag, which is the closed-loop lag of the even subframe minus 18 (9.3.3 step 5); for each odd subframe the 2-bit offset index LO; for each subframe the pitch gain index GI. The fixed codebook document supplies FG, the grid bit, the pulse positions, the signs and the pulse train bit for the GAIN word of section 11 and for the rest of the bitstream.

## 13. Where the reference differs from or adds to the text

- No sample scaling anywhere (section 1).
- The Levinson-Durbin guard, PK2 and the sine detector (4.3, 4.5) are not in the text. The detector flag also restricts the pitch gain search (9.4.2).
- LPC to LSP: the silent fallback to the previous quantized LSP vector when fewer than 10 roots are found (5.1).
- LSP stability check: the relaxed threshold S - 1/32, the clamps at 3.0 and 252.0, and the sequential correction of every close pair (5.3). The erased-frame variant is the same procedure with the index replaced by 0, S = 4.0 and P = 23/32.
- Open-loop pitch: the 1.25 dB test is the factor 4/3 realized as D > 0.25 C2 Ebest (section 7).
- Harmonic noise shaping: the 2.0 dB test of the text is the fixed threshold 0.375 (8.1).
- Pitch predictor: the clamping of the open-loop lag to 19 to 140 for even subframes; the indexing of the periodic extension in equation 41.1 (n - 2 rather than n); the limit of the gain search by taming; the table choice by the lag of the preceding even subframe in odd subframes (9.3).
- The taming procedure and ERR0 in the initial state (2, 9.4).
- `WEIGHTING_FILTER_ZERO_FACTORS` entry 6 holds 0.4781969 in the reference and is used as it is.
- The excitation history is clipped to the 16-bit range with thresholds -32767.5 and 32766.5 and not rounded; the filter memory update uses the unclipped excitation (9.5, 9.6).

## 14. Ambiguities and gaps

- The sine flag is tested in the reference as a signed 16-bit value below zero. Implement the register as an unsigned 16-bit value and test bit 15.
- The fixed-point reference computes the zone indices of the taming test (9.4.2) as the product of the lag and 1092 shifted right by 15 bits, which differs from integer division by 30 when the argument is a multiple of 30. The floating-point reference, which this document follows, uses integer division by 30. The taming update (9.4.3) uses the shifted product in both references.
- Section 7 requires the recursive energy update. A direct recomputation of each energy differs in the last bits and can change a comparison.
- The description of what the entries of the taming vector stand for, described in 9.4, is an interpretation. The rules that follow it are taken from the reference and are complete.
- The reference build options were not available beside the source; the choice of double precision in section 1 follows the default of the type definition.
- `tables.md` lacks no table that this document needs. Scalar constants (ERR0, 128.0, 256.0, 51, 93, 85, 170, 58, 0.375, 0.3125, 0.25, 0.95, 0.99) appear in the text of the sections that use them.
