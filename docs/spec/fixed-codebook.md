# Fixed codebook excitation: encoder search, parameter coding, decoder reconstruction

This document specifies, for the ITU-T G.723.1 codec of the 05/2006 edition with Annex B (floating-point reference):

- the high rate (6.3 kbit/s) multi-pulse excitation, called MP-MLQ in the Recommendation (clause 2.15),
- the low rate (5.3 kbit/s) algebraic excitation, called ACELP (clause 2.16),
- the way the pulse positions, signs, grid bit and gain index are placed in the 12-bit GAIN, POS, MSBPOS and PSIG fields (clauses 2.17, 2.18, 2.20, Tables 2 and 4 to 6),
- the decoder side reconstruction of the fixed excitation (clause 2.17, repeated as 3.5) and its addition to the adaptive codebook contribution,
- the encoder side reconstruction of the total excitation after the search, which feeds the memory update (clause 2.19).

Tables are named as in `docs/spec/tables.md`. The Recommendation text gives only a summary of these procedures. Where the floating-point reference differs from the text or adds to it, the paragraph says so under the words "The text does not say" or "The reference differs". An implementation that follows the reference rules below is the target. Section 13 collects the differences in one list.

## 0. Conventions

### 0.1 Numbers

- The reference defines its floating-point type as double precision (64-bit) unless a build option for single precision is given. This document assumes double precision. No procedure of this document rounds to single precision. Integer-valued quantities (positions, indices, counters) are exact integers.
- One constant is a single-precision limit used as a double: the smallest normal single-precision number, 1.17549435e-38. It appears once, in the low rate gain computation (section 8.5).
- The reference stores the amplitude levels in FIXED_GAIN_LEVELS as decimal constants. Their unit is the unit of the target vector, so a pulse of the high rate excitation has the amplitude plus or minus the level itself and no further scaling is applied.

### 0.2 Sums

A subframe has 60 samples, numbered 0 to 59. Unless a rule states a different order, every sum of products over an index range is evaluated as a running sum that starts at zero and adds the terms in increasing order of the summation index, one term at a time. The notation inner(a, b) for two vectors of length 60 means the sum over n = 0 to 59 of a[n] times b[n] in that order. The notation inner(a shifted by i, b, 60 - i) means the sum over n = 0 to 59 - i of a[i + n] times b[n].

Floating-point sums are not associative. The orders given in this document are the orders of the reference. An implementation that uses another order will differ in the last bits of intermediate values, and this can change a decision when two candidates are almost equal. The conformance criterion tolerates a small share of such differences, but the orders below should be followed where they are cheap to follow.

### 0.3 Names of the inputs

For one subframe i (0 to 3) the fixed codebook stage receives:

- the target vector r[n], n = 0 to 59. It is the perceptually weighted speech of the subframe after the zero input response (clause 2.13) and the five-tap pitch predictor contribution (clause 2.14, equation 20) have been subtracted. At the high rate it is called "the residual signal" in clause 2.15. The adaptive codebook search of the same subframe leaves it in the target buffer, so no further subtraction is needed,
- the impulse response h[n], n = 0 to 59, of the combined filter (synthesis filter, formant weighting filter and harmonic noise shaping filter in cascade, clause 2.12),
- the base pitch lag L of the subframe pair. It is the lag value that is finally transmitted for the even subframe of the pair, that is the closed-loop lag of subframe 0 for subframes 0 and 1 and the closed-loop lag of subframe 2 for subframes 2 and 3. For odd subframes it is the lag of the preceding even subframe, not the lag of the odd subframe itself. The text calls it L0 or L2,
- the lag offset code AcLg of the subframe (the 2-bit differential lag code of odd subframes, and the value 1 for even subframes),
- the pitch gain codebook index AcGn of the subframe (0 to 169, from the adaptive codebook search; clause 2.14),
- the coder rate.

The actual closed-loop lag of subframe i is L - 1 + AcLg. For even subframes it equals L.

### 0.4 Outputs

For one subframe the stage produces five integer parameters and the fixed excitation vector. The parameters are:

- the position index (a number of up to 20 bits at the high rate, 12 bits at the low rate),
- the sign word (6 or 5 bits at the high rate, 4 bits at the low rate),
- the amplitude index, an index into FIXED_GAIN_LEVELS (0 to 23),
- the grid bit (0 or 1),
- the pulse train bit (high rate only, 0 at the low rate).

The fixed excitation vector is the vector of 60 samples that is added to the adaptive contribution in section 11. It replaces the target vector in memory, because the reference overwrites the target by it.

### 0.5 Pitch lag threshold

Both rates use the number 58 (60 minus 2) as the limit below which a lag is "short". At the high rate the test is L < 58 on the base lag. At the low rate the test is T < 58 on the modified lag T of section 8.1. The text says "60" in clause 2.16 for the low rate. The reference uses 58 for both.

## 1. Parameter conventions common to both rates

### 1.1 Gain word (12 bits)

The 12-bit GAIN parameter of subframe i is built from the pitch gain index AcGn (clause 2.14), the amplitude index MG (0 to 23) and, at the high rate with a short lag, the pulse train bit. Let GSize = 24.

Encoder:

1. word = AcGn times 24 plus MG.
2. At the high rate only, the pulse train bit is added as the value 2048 times the pulse train bit. The reference does this for every subframe at the high rate. For L >= 58 the bit is always 0, so the word equals AcGn times 24 plus MG. For L < 58, AcGn is below 85, so AcGn times 24 plus MG is below 2040 and does not collide with bit 11.
3. At the low rate there is no pulse train bit. AcGn is below 170, so the word is below 4080.

Decoder (equations 36, 39 and 40 of the text), applied to the word read from the bitstream, with L the base lag of the subframe:

1. If the rate is high and L < 58: the pulse train bit is the word divided by 2048 (integer division, that is bit 11). The word is then replaced by itself with bit 11 cleared (word AND 2047). The bound for AcGn is 85.
2. Otherwise the pulse train bit is 0, the whole 12-bit word is used and the bound for AcGn is 170.
3. AcGn = word divided by 24 (integer division, equation 39 and 40). MG = word minus AcGn times 24, that is the word modulo 24 (equation 36).
4. If AcGn is not below the bound of step 1 or 2, the index is invalid. See section 10.

For the low rate the rule of step 2 always applies.

MG is always a valid index (0 to 23), because it is a remainder modulo 24.

The decoder decides between the 85-entry and the 170-entry pitch gain codebook with the same L (clause 2.14). That choice is described in the adaptive codebook document.

### 1.2 Grid bit

One bit per subframe. It is the shift of all pulses by one sample. At the high rate it selects between all-even positions (0) and all-odd positions (1) on a grid of 30 positions of spacing 2. At the low rate it adds 1 to the sample position of every pulse.

### 1.3 Where the fields sit in the frame

Section 9 gives the order of the fields in the 192-bit (high rate) or 160-bit (low rate) frame body and the numbering of the bits.

## 2. High rate excitation: structure (clause 2.15)

The excitation of one subframe is M equal-magnitude pulses, with signs, at distinct positions of one grid:

- M = MPMLQ_PULSE_COUNTS[i]: 6, 5, 6, 5 for subframes 0 to 3,
- the grid with grid bit k has the 30 sample positions k, k + 2, ..., k + 58; grid slot j (0 to 29) is the sample k + 2 j,
- every pulse has the same magnitude, the level FIXED_GAIN_LEVELS[MG] chosen by the amplitude index MG,
- if the pulse train bit is 1 (only possible for L < 58), the excitation actually used is the pulse vector repeated with period L, in the sense of section 4.

So the excitation, for the plain case, is v[n] = sum over the M pulses of (sign times level) times a unit sample at the pulse position. The text writes G and alpha_k for level and signs (equation 22).

## 3. High rate excitation: calls from Find_Fcbk

For a subframe i at the high rate, Find_Fcbk does:

1. Set the best error found so far to -99999999.9. Set M = MPMLQ_PULSE_COUNTS[i].
2. Call Find_Best with the argument value P = 60 (no pulse train). Because P is not below 58, Find_Best works with the impulse response itself.
3. If L < 58, call Find_Best again with P = L (pulse train variant). It keeps the result of the first call as the best so far and replaces it only if it finds a strictly better one (section 5.6).
4. Rebuild the plain pulse vector from the best result, code it (section 7) and, if the best result is a pulse train result, expand it (section 4).

Both calls use the same r[n] and h[n] and neither changes them.

## 4. Pulse train (Gen_Trn)

Gen_Trn turns a vector s[n] and a period P into the vector y[n] defined by:

y[n] = s[n] + s[n - P] + s[n - 2 P] + ... , where only terms with index at least 0 are present.

In order: y starts as a copy of s. Then for the multiples m P = P, 2 P, 3 P, ... that are below 60, in increasing order, y[n] is increased by s[n - m P] for n = m P to 59. The values added are always taken from the unchanged s, never from partial results of y. (The reference makes a copy of s first so that the same call is correct when s and y are the same array.) This is a plain sum of copies of s shifted by multiples of P, not a recursive comb filter.

Gen_Trn is used:

- on the impulse response, with period P, inside Find_Best when P < 58 (section 5, step 1),
- on the final pulse vector, with period L, after the search, when the selected result is a pulse train result (section 3, step 4, and the decoder, section 10.2).

Equivalence: the error of equation 23 computed with the train-modified impulse response and unmodified pulses equals the error computed with the expanded excitation and the plain impulse response, because convolution commutes with the sum of shifts. The reference searches with the modified impulse response and expands the excitation afterwards.

## 5. High rate excitation: Find_Best

Inputs: r[n], h[n], M, and P (60 or L). The best-so-far record (error value, grid bit, amplitude index, pulse train flag, M positions, M signed amplitudes) is shared with the caller.

### 5.1 Preparation

1. If P < 58: the pulse train flag of this call is 1 and the working impulse response g[n] is Gen_Trn applied to h with period P (section 4). Otherwise the flag is 0 and g[n] = h[n].
2. Autocorrelation of g: c[i] = inner(g shifted by i, g, 60 - i), for i = 0 to 59. That is c[i] = sum over n = 0 to 59 - i of g[n] g[n + i]. The value c[0] is the energy of g.
3. Correlation of the target with g: e[i] = inner(r shifted by i, g, 60 - i), for i = 0 to 59. That is e[i] = sum over n = 0 to 59 - i of r[n + i] g[n]. This is d[j] of equation 24 computed with g in place of h.

### 5.2 Loop over grids and gains

The loop order is: grid bit k = 0, then k = 1. For each grid, the gain candidates in increasing order of amplitude index (section 5.4). For each (grid, gain) pair, one sequential pulse search is run (section 5.5) and its error is compared with the best so far (section 5.6).

### 5.3 First pulse position and gain estimate (once per grid)

1. Scan the 30 positions p = k, k + 2, ..., k + 58 in increasing order. Keep the position with the largest |e[p]|. The comparison is "greater than or equal to", starting from the value 0, so a later position replaces an earlier one with an equal magnitude. When all |e[p]| on the grid are 0, the position is k + 58. Call this position m0 and its magnitude A.
2. A is the estimate of the largest correlation of equation 25 (Gmax times the energy of g), now taken as the largest magnitude, not the largest signed value, and taken for this grid.
3. Quantize it. Let delta(q) = | FIXED_GAIN_LEVELS[q] times c[0] - A |. Visit q = 22, 21, 20, ..., 2 (22 down to 2 inclusive) in that order. The running minimum starts at the value 32767.0 and a visited q replaces the current choice only if delta(q) is strictly smaller than the running minimum. The initial choice is q = 22. Call the final choice q*. (So among equal deltas the largest q wins. If every delta is 32767.0 or more, q* stays 22.) The range 2 to 22 guarantees that the candidates of the next step stay inside 0 to 23. The comparison is between the level times the energy c[0] and the correlation magnitude A, not between the level and a gain estimate A divided by c[0].

### 5.4 Gain candidates

The amplitude indices tried for the grid are, in this order: q* - 2, q* - 1, q*, q* + 1 (four values). The text says the gains cover "[Gmax - 3.2, Gmax + 6.4]" dB, which would be one step below to two steps above. The reference differs: it covers two steps below to one step above (about -6.4 dB to +3.2 dB). The amplitude of a candidate is FIXED_GAIN_LEVELS[index].

### 5.5 Sequential pulse search for one grid and one gain

Let a be the amplitude (the level of the candidate). Work with a vector w[p] defined on the 30 grid positions, and a set of occupied positions.

1. Set w[p] = e[p] for all 30 positions p of the grid. Set the occupied set to empty.
2. Pulse 0 is at position m0 of section 5.3. Its amplitude is +a if w[m0] >= 0 and -a otherwise (zero gives the positive sign). Mark m0 occupied.
3. For pulse j = 1 to M - 1, in order:
   1. For every grid position p that is not occupied, in increasing order: replace w[p] by w[p] minus (amplitude of pulse j - 1) times c[|p - position of pulse j - 1|]. Occupied positions are not changed. (So the correlation is corrected for one more pulse at every step, always by one subtraction per pulse and in pulse order.)
   2. While doing so, track the unoccupied position with the largest |w[p]| (the value after replacement). The tracking starts at the value -32768.0 and a position replaces the current choice only if its |w[p]| is strictly greater. Hence the first (lowest) position wins ties, and the choice always exists.
   3. Pulse j is at the chosen position. Its amplitude is +a if the updated w at that position is >= 0, otherwise -a.
   4. Mark the position occupied.
4. The result is the list of M positions and M signed amplitudes, in the order of selection (not sorted).

### 5.6 Error of a candidate and selection

1. Build the 60-sample vector u that is zero except for the signed amplitudes at the M positions.
2. Convolve with g: s[l] = sum over j = 0 to l of u[j] g[l - j], for l = 0 to 59, each sum with j increasing. The convolution is truncated at sample 59.
3. The error criterion (to be maximized) is E = 2 times inner(r, s) minus inner(s, s). It is the quantity of equation 23 with the constant energy of r removed, so a larger E means a smaller squared error.
4. If E is strictly greater than the best error so far, the candidate becomes the best: the best error, the grid bit k, the amplitude index, the flag of this call and all M positions and amplitudes are stored. With "strictly greater", the first candidate in the order of section 5.2 wins ties, and the call without pulse train is preferred to the call with pulse train.

The candidate order across the two calls of section 3 is therefore: no train (grid 0, four gains; grid 1, four gains), then with train (grid 0, four gains; grid 1, four gains).

## 6. High rate: result of the search

After the calls of section 3 the best result gives the grid bit, the amplitude index, the pulse train flag and the M signed amplitudes at their positions. Rebuild the vector v[n]: all 60 samples zero, then v[position] = signed amplitude for each pulse.

This is the plain pulse vector, before any expansion by Gen_Trn. If the flag of the best result is 1, the plain vector is what is coded in section 7 and Gen_Trn is applied to it afterwards.

## 7. High rate: combinatorial coding of positions and signs (Fcbk_Pack)

The codebook has the size C(30, M) = MPMLQ_CODEBOOK_SIZES[i] (593775 for M = 6, 142506 for M = 5). A selection of M slots out of the 30 slots of the grid gets a number from 0 to size - 1, and the signs go in a separate word. Let T(i, j) = MPMLQ_COMBINATORIAL_TABLE row i, column j: the number of ways to choose 5 - i items from a pool of 29 - j items, and 0 where 5 - i is larger than 29 - j (so that T(i, j) = C(29 - j, 5 - i)).

Procedure (the vector v of section 6 is examined slot by slot):

1. Set the row counter rho = 6 - M (0 for M = 6, 1 for M = 5). Set position index = 0 and sign word = 0.
2. For slot j = 0, 1, ..., 29 in this order, look at the sample v[k + 2 j], with k the grid bit:
   1. If the sample is exactly 0 (no pulse in the slot): increase the position index by T(rho, j).
   2. Otherwise (a pulse): replace the sign word by two times the sign word plus 1 if the pulse is negative or plus 0 if it is positive. Then increase rho by 1. If rho is now 6, stop.
3. The amplitude index is the index selected in the best result, the grid bit is the selected grid bit, and the pulse train bit is the flag of the best result.

Notes:

- Slots after the last pulse add nothing. The count of empty slots in front of each pulse enters through the table row that corresponds to the number of pulses still to place.
- The sign word contains one bit per pulse, M bits in all (6 for M = 6, 5 for M = 5). The pulse in the lowest slot gives the most significant bit and the pulse in the highest slot gives the least significant bit. A bit of 1 means a negative pulse.
- The position index of an arrangement is always smaller than the codebook size.

Decoding (section 10.2) inverts the procedure.

## 8. Low rate excitation (ACELP, clause 2.16)

### 8.1 Pitch-synchronous parameters

Let L be the base lag, AcLg the offset code, AcGn the pitch gain index, as in section 0.3.

1. The lag of the subframe is Ls = L - 1 + AcLg.
2. T = Ls + ACELP_PITCH_TAP_OFFSETS[AcGn]. The table entry is the integer e of the text (between -2 and 2), or 60 for entries that add nothing; with 60 the sum T is far above 58 and the following rules do not apply.
3. beta = ACELP_PITCH_TAP_GAINS[AcGn] (the b of the text).
4. The pitch-synchronous contribution is active if and only if T < 58.

The text speaks of "pitch delays less than 60". The reference tests T < 58 on the modified lag T.

### 8.2 Modification of the impulse response

If the contribution is active, replace the impulse response in place, for n = T, T + 1, ..., 59 in increasing order, by h[n] = h[n] + beta times h[n - T]. Because the replacement is in place and in increasing order, the term h[n - T] is already the modified value when n - T >= T. The modification is therefore recursive (it creates echoes at 2 T, 3 T, ... with weights beta squared, beta cubed, ...). The text writes the modification only once. The reference is recursive.

All the following computations of the low rate search (d, Phi, filtered codeword, gain) use this modified impulse response. In the description below, h[n] always means the modified impulse response.

### 8.3 Correlation of the target with the impulse response (Cor_h_X)

d[i] = inner(r shifted by i, h, 60 - i), for i = 0 to 59 (equation 28). Set d[60], d[61], d[62], d[63] = 0.

The target r is used here as it stands. The search later flips signs inside its working copy of d only. r itself is not changed until the final vector overwrites it (section 8.9).

### 8.4 Covariance matrix (Cor_h)

For two sample positions p and q (0 to 63) define Phi(p, q) as the sum over n from max(p, q) to 59 of h[n - p] h[n - q] (equation 29), which is 0 when max(p, q) is 60 or more. Only positions of the form 8 x + 2 k (x = 0 to 7, k = 0 to 3) are needed: pulse number k (0 to 3) can sit at the positions 8 x + 2 k, so the positions are 0, 8, ..., 56 for pulse 0, 2, 10, ..., 58 for pulse 1, 4, 12, ..., 60 for pulse 2, 6, 14, ..., 62 for pulse 3. The positions 60 and 62 lie outside the subframe, which means that the pulse is not present, and Phi is zero there. Table 1 of the Recommendation is exactly this set of positions, defined here by the formula 8 x + 2 k.

Summation rule for Phi(p, q), with p <= q after swapping if needed (Phi is symmetric): Phi(p, q) = sum over u = 0 to 59 - q of h[u] h[u + q - p]. The sum is a running sum from zero that adds two terms at a time, as the pair (term u plus term u + 1) added to the running sum, for u = 0, 2, 4, .... The number of terms 60 - q is always even because q is even. For p = q this reads Phi(p, p) = sum over u = 0 to 59 - p of h[u] times h[u].

The reference stores them as ten tables: Phi(p, p) for each pulse number, and Phi(p_a, p_b) for the six pairs of pulse numbers (a, b) = (0,1), (0,2), (0,3), (1,2), (1,3), (2,3), with p_a taking the eight positions of pulse a and p_b the eight positions of pulse b. Both orderings (p_a greater than p_b and p_a smaller than p_b) occur, and both use the symmetric value above.

### 8.5 Search preparation (D4i64_LBC)

1. Pair signs. For j = 0 to 29: the pair of samples (2 j, 2 j + 1) gets the sign S_j = +1 if d[2 j] + d[2 j + 1] >= 0, and S_j = -1 otherwise. (A sum of exactly 0 gives +1.) For j = 30 and 31 (samples 60 to 63) S_j = +1. The vector d is changed in place: d'[n] = S_j d[n] for n = 2 j and 2 j + 1, j = 0 to 29. The entries d[60] to d[63] stay 0. All following uses of d mean d'.

   The text (equation 32) selects the sign by comparing d[2 j] and d[2 j + 1] and taking the sign of the larger one. The reference uses the sign of the sum, as above. A pulse at an odd sample (shift 1) uses the sign of the pair that contains it, which is the same pair as for the even sample before it.

2. Sign-folded matrix. For the six pulse-pair tables, the entry for sample positions (p_a, p_b) is replaced by 2 S(p_a) S(p_b) Phi(p_a, p_b), where S(p) = S_(p divided by 2, integer part) is the pair sign of the sample p. The factor 2 of the cross terms in equation 31 is built in. This value is written folded(p_a, p_b) below. The four diagonal tables Phi(p, p) are not changed.

   With these definitions, the correlation and energy of a candidate with pulses at the unshifted sample positions p_0, p_1, p_2, p_3 (pulse number k at 8 x_k + 2 k) and shift s are:

   C = d'[p_0 + s] + d'[p_1 + s] + d'[p_2 + s] + d'[p_3 + s],

   energy = (sum over k of Phi(p_k, p_k)) + (sum over the pairs a < b of the folded entry of the positions p_a and p_b).

   A shifted candidate uses the Phi values of the unshifted positions. This is the approximation of the text for odd position codevectors. The pair sign of p and p + 1 is the same, so the folded entries are valid for both shifts.

3. Threshold (equations 32 to 35 of the text, "thr3"). The reference does it for both shifts and takes the larger value, as follows. For the shift o (o = 0 first, then o = 1):

   1. max_0 = the largest of d'[o], d'[o + 8], ..., d'[o + 56]. max_1 = the largest of d'[o + 2], d'[o + 10], ..., d'[o + 58]. max_2 = the largest of d'[o + 4], d'[o + 12], ..., d'[o + 60]. (Each is the maximum over 8 values, found with the comparison "strictly greater", which makes no difference to the value. The last value of max_2 is d'[60] = 0 for o = 0 or d'[61] = 0 for o = 1, because the vector is extended with zeros.) Values are signed; there is no absolute value.
   2. maximum = max_0 + max_1 + max_2 (in this order).
   3. average: the running sum, for i = o, o + 8, ..., o + 56 in order, of ((d'[i + 4] + d'[i + 2]) + d'[i]); added to the running sum which starts at zero. Then multiplied by 0.125. (So it is the mean over the 8 slots of the sum of the three values.)
   4. thr(o) = average + (maximum - average) times 0.5.

   The threshold used is thr3 = thr(0) if thr(1) is not greater than thr(0), otherwise thr(1).

   The text speaks of the "absolute correlation" and of one threshold. The reference uses signed values of d' (which are the sign-aligned correlations) and the larger of the two shifts' thresholds.

### 8.6 Search loops

The search runs over the pulse slots x_0, x_1, x_2, x_3, each from 0 to 7, as four nested loops, with x_0 outermost and x_3 innermost, all in increasing order. The sample position of pulse k at slot x is 8 x + 2 k (no shift). The initial state of the best candidate is: slots (0, 1, 2, 3) meaning sample positions 0, 2, 4, 6, shift 0, best correlation-squared 0.0 and best energy 1.0. The budget of the search is described in section 8.7.

In the loops, with partial sums built in the order shown (the parentheses show the order of addition, which must be kept for bitwise agreement):

1. Loop over x_0: position p_0 = 8 x_0. A0 = d'[p_0] (shift 0 correlation), A0' = d'[p_0 + 1] (shift 1). Energy E0 = Phi(p_0, p_0).
2. Loop over x_1: position p_1 = 8 x_1 + 2. A1 = A0 + d'[p_1]. A1' = A0' + d'[p_1 + 1]. Energy E1 = (E0 + Phi(p_1, p_1)) + folded(p_0, p_1).
3. Loop over x_2: position p_2 = 8 x_2 + 4. A2 = A1 + d'[p_2]. A2' = A1' + d'[p_2 + 1]. Energy E2 = ((E1 + Phi(p_2, p_2)) + folded(p_0, p_2)) + folded(p_1, p_2). Then decide the shift: if A2' is strictly greater than A2, the shift is 1 and A2 is replaced by A2'. Otherwise the shift is 0.
4. If A2 (after the replacement) is strictly greater than thr3, run the loop over x_3. Otherwise this value of x_2 is skipped and the innermost loop is not entered.
5. Loop over x_3 (only when entered): position p_3 = 8 x_3 + 6. A3 = A2 + d'[p_3 + shift]. Energy E3 = (((E2 + Phi(p_3, p_3)) + folded(p_0, p_3)) + folded(p_1, p_3)) + folded(p_2, p_3). Let Q = A3 times A3. The candidate becomes the best if Q times (best energy) is strictly greater than (best correlation-squared) times E3. In that case the best is updated: best correlation-squared = Q, best energy = E3, the four unshifted positions p_0 to p_3, and the shift.
6. After the whole loop over x_3 has finished for this (x_0, x_1, x_2), the search budget is reduced by 1 (section 8.7). If the budget is then 0 or less, the whole search stops at once: the remaining loops at every level are abandoned.

The comparison in step 5 is the cross-multiplied form of "C squared divided by energy is greater". Because it is strictly greater and the first candidate is tested against (0.0, 1.0), a candidate with A3 = 0 never replaces the initial state, and among equal criteria the first candidate in loop order is kept. If no candidate ever replaces the initial state, the result is the initial state: slots (0, 1, 2, 3), shift 0.

### 8.7 Search budget (the "600" limit)

The text says the last loop is entered at most 600 times for the four subframes. The reference implements this with a budget counter that persists from one subframe to the next inside a frame:

1. At the start of every frame that is coded at the low rate, before the first subframe, a carry-over value is set to 120. This is done by the main program for each such frame, whether or not the frame is later found to be active.
2. At the start of the search of each subframe, the budget is 120 plus the carry-over value.
3. In step 6 of section 8.6 the budget is reduced by 1 after each full run of the innermost loop; the search stops when it is 0 or less. The innermost loop that has started always runs to its end before the stop.
4. When the search of the subframe ends, whether by exhaustion of the loops or by the stop, the carry-over value is set to the remaining budget (exactly 0 after a stop, possibly large after a complete search).

The first subframe can therefore enter the innermost loop up to 240 times, and unused budget is passed on. With four subframes the total is at most 120 plus 4 times 120 = 600, as the text says. The carry-over is not kept from one frame to the next, it starts again at 120. The carry-over is not touched at the high rate.

### 8.8 Selected codeword

Let the best candidate have unshifted positions p_0 to p_3, shift s, and let q_k = p_k + s.

1. Pulse signs: sign_k = S(p_k), the pair sign of the unshifted position (the same as the pair sign of q_k). Pulses 2 and 3 can have p_k = 60 or 62; there S = +1.
2. Unit pulse vector u: all 60 samples zero, then u[q_k] = sign_k for each pulse with q_k < 60 (a pulse at q_k >= 60 is absent). The four positions are different, so the order of placing does not matter.
3. Filtered codeword y[n], n = 0 to 59: start with all zero; then for pulse k = 0 to 3 in order, for each pulse with q_k < 60: for n = q_k to 59, y[n] = y[n] + h[n - q_k] if sign_k is positive, and y[n] = y[n] - h[n - q_k] if negative. The modified impulse response of section 8.2 is used.
4. Sign word: the sum over k of 2^k times (1 if sign_k is positive, 0 if negative). Bit k of the word belongs to pulse k. A set bit means positive. This is the opposite of the high rate, where 1 means negative. The bits of pulses 2 and 3 are coded as they are, also when the pulse is absent (then the bit is 1, from S = +1).
5. Position index: the sum over k of 8^k times (integer part of q_k divided by 8). This is the slot x_k of each pulse in 3 bits, pulse 0 in the three lowest bits and pulse 3 in bits 9 to 11, 12 bits in all. The slot 7 of pulse 2 gives the sample 60 or 61 and the slot 7 of pulse 3 gives the sample 62 or 63. These samples are outside the subframe, so the pulse is absent.
6. Grid bit (the "shift" of the text): s.

### 8.9 Gain (G_code) and final vector

1. xy = inner(r, y) with the unchanged target r and the filtered codeword y of step 3 above.
2. If xy <= 0: the amplitude index is 0 and the quantized gain is FIXED_GAIN_LEVELS[0]. Go to step 6.
3. yy = inner(y, y). If yy > 1.17549435e-38 the unquantized gain is xy divided by yy. Otherwise it is 0.
4. Choose the amplitude index as the index q for which |unquantized gain - FIXED_GAIN_LEVELS[q]| is smallest, visiting q = 0, 1, ..., 23 in order. A later q replaces the current choice only if its distance is strictly smaller, so the lowest index wins ties. Start from q = 0.
5. The quantized gain is FIXED_GAIN_LEVELS of that index. (This is the quantization of the text "stepping through the gain quantization table ... minimizing |G - G_j|".)
6. The fixed excitation vector of the subframe: v[n] = quantized gain times u[n], for n = 0 to 59.
7. If the pitch-synchronous contribution is active (T < 58): for n = T, T + 1, ..., 59 in increasing order, v[n] = v[n] + beta times v[n - T]. As in section 8.2 this is done in place, so the added term is the already modified value (recursive).
8. This vector replaces the target in memory.

Outputs to the parameter set: position index (section 8.8 step 5), sign word, amplitude index, grid bit. The pulse train bit is 0.

Note that the unquantized gain is for the unit pulse vector filtered by the modified h, which includes the pitch-synchronous part. Therefore the quantized gain applies to the unit pulses before the pitch-synchronous addition of step 7.

## 9. Placement in the bitstream (Tables 2, 4, 5, 6; clause 2.20)

A packed frame is a sequence of bits numbered from 0. Bit b of the sequence goes to bit (b modulo 8) of octet (b divided by 8, integer part), where bit 0 of an octet is its least significant bit. Tables 5 and 6 of the text write each octet with the most significant bit on the left, so the first bit sent of a field appears at the right end of the first octet it touches. Every multi-bit field is sent with its least significant bit first (PARx_B0 first).

The fields, in the order in which they are sent, are the following. The fields that are not part of the fixed codebook are listed only to fix the position of the others.

High rate (192 bits, 24 octets):

1. the two bits that tell the rate and the activity (2 bits),
2. the LSP index (24 bits) and the lag fields ACL0 (7 bits), ACL1 (2 bits), ACL2 (7 bits), ACL3 (2 bits),
3. the four gain words of 12 bits, for subframes 0, 1, 2, 3 (GAIN0 to GAIN3, section 1.1),
4. the four grid bits, for subframes 0, 1, 2, 3 (GRID0 to GRID3, one bit each),
5. one unused bit with the value 0 (UB in Table 5, octet 13),
6. MSBPOS, 13 bits (section 9.1),
7. the low part of the position index of subframe 0 (16 bits), of subframe 1 (14 bits), of subframe 2 (16 bits) and of subframe 3 (14 bits). Table 5 calls these POS0 to POS3 (POS0_B15 to POS0_B0 and so on),
8. the sign words: 6 bits for subframe 0, 5 bits for subframe 1, 6 bits for subframe 2, 5 bits for subframe 3 (PSIG0 to PSIG3).

The total is 2 + 24 + 18 + 48 + 4 + 1 + 13 + 60 + 22 = 192.

Low rate (160 bits, 20 octets):

1. to 4. as for the high rate (rate and activity bits, LSP index and lags, the four gain words, the four grid bits),
5. the four position indices of 12 bits each, for subframes 0 to 3 (POS0 to POS3 in Table 6),
6. the four sign words of 4 bits each, for subframes 0 to 3 (PSIG0 to PSIG3).

The total is 2 + 24 + 18 + 48 + 4 + 48 + 16 = 160.

### 9.1 MSBPOS (high rate)

The position index of subframes 0 and 2 (M = 6) is below 593775 and needs 20 bits. The position index of subframes 1 and 3 (M = 5) is below 142506 and needs 18 bits. The low 16 bits (subframes 0 and 2) or low 14 bits (subframes 1 and 3) are sent as POS0 to POS3. The remaining upper parts are

- H0 = position index of subframe 0 divided by 65536 (integer division), a number from 0 to 9,
- H1 = position index of subframe 1 divided by 16384, from 0 to 8,
- H2 = position index of subframe 2 divided by 65536, from 0 to 9,
- H3 = position index of subframe 3 divided by 16384, from 0 to 8.

The text calls them "the four most significant bits" (Table 4 note). They are combined into one number by mixed radix:

MSBPOS = (H0 times 9 + H1) times 90 + H2 times 9 + H3.

The largest value is (9 times 9 + 8) times 90 + 9 times 9 + 8 = 8099, which fits in 13 bits (the number of values 8100 is below 8192). The three bits saved by this combination are the "3 additional bits" of the text. MSBPOS is sent as a 13-bit number, least significant bit first. In Table 5 it is MSBPOS_B0 to MSBPOS_B12, over octets 13 and 14.

The low parts: POS0 = position index of subframe 0 AND 65535 (16 bits), POS1 = position index of subframe 1 AND 16383 (14 bits), POS2 = position index of subframe 2 AND 65535, POS3 = position index of subframe 3 AND 16383. Each is sent least significant bit first, in the order of the list above.

Decoding: let A = MSBPOS divided by 90 (integer) and B = MSBPOS minus 90 times A. Then H0 = A divided by 9 (integer), H1 = A minus 9 times H0, H2 = B divided by 9 (integer), H3 = B minus 9 times H2. The position index of subframe 0 is H0 times 65536 plus POS0; of subframe 1: H1 times 16384 plus POS1; of subframe 2: H2 times 65536 plus POS2; of subframe 3: H3 times 16384 plus POS3. The reference does not check that MSBPOS is below 8100 here. Values 8100 to 8191 give H0 above 9 and then a position index that is too large (section 10.2).

### 9.2 Sign words at the high rate

PSIG of subframe i is the sign word of section 7, with M bits. The least significant bit is the sign of the pulse with the highest slot (the last pulse in order of position), and it is sent first. The most significant bit is the sign of the pulse with the lowest slot. In Table 5: PSIG0_B5 is the most significant bit.

### 9.3 Low rate

The 12-bit position index has pulse 0 in bits 0 to 2 and pulse 3 in bits 9 to 11, as described in section 8.8. The 4-bit sign word has pulse 0 in bit 0 and pulse 3 in bit 3.

## 10. Fixed codebook decoding (clause 2.17, Fcbk_Unpk)

The inputs are the five parameters of the subframe (section 0.4), the base lag L, the lag offset code AcLg, the pitch gain index AcGn (after the unpacking of section 1.1) and the subframe number i. The output is the fixed excitation vector of 60 samples. In the decoder the amplitude is the level FIXED_GAIN_LEVELS[MG].

### 10.1 Steps of the text and where they are

The text lists the steps: derive MG from the gain index (equation 36 and section 1.1), decode the positions (combinatorial for the high rate, direct for the low rate), derive the grid position from the grid bit, the signs from the sign bits, decode the pulse train bit only if L < 58 (high rate), reconstruct v[n], and add the pitch contribution to get e[n] (section 11).

### 10.2 High rate

1. Set all 60 samples of the output to 0.
2. M = MPMLQ_PULSE_COUNTS[i]. If the position index is not below MPMLQ_CODEBOOK_SIZES[i], the index is invalid: leave the output at 0 (no pulses, no pulse train expansion) and stop. There is no error flag and no other consequence in the reference. The adaptive contribution is still added by the caller.
3. Otherwise set the row counter rho = 6 - M and the remainder R = position index.
4. For slot j = 0, 1, ..., 29 in order:
   1. R = R - T(rho, j).
   2. If R is negative: set R = R + T(rho, j) (undo). Increase rho by 1. The slot holds a pulse. The sample k + 2 j (k the grid bit) is set to minus the level if the bit of the sign word with the value 2^(6 - rho) is 1, otherwise to plus the level. (Here rho is the value after the increase. For M = 6 the first pulse uses the bit 2^5, the most significant bit, and the last pulse uses 2^0. For M = 5 the first pulse uses 2^4 and the last 2^0, because rho started at 1.) If rho is now 6, stop the loop.
5. If the pulse train bit is 1, apply Gen_Trn (section 4) to the vector with the period L, in place.

The pulse train bit is only nonzero in the word when L < 58 (section 1.1). The decoder does not test L again here: it applies Gen_Trn whenever the bit is 1. With a valid stream the bit is 1 only for L < 58.

A valid index is always decoded completely: the loop ends with exactly M pulses. For an index that is below the codebook size, the pulses found are the same as those the encoder chose.

### 10.3 Low rate

1. A temporary vector of 64 samples is set to zero.
2. For pulse k = 0 to 3: slot field = the bits 3 k to 3 k + 2 of the position index (value 0 to 7). Sample position q_k = 8 times slot field plus the grid bit plus 2 k. The value written at q_k is plus the level if bit k of the sign word is 1, and minus the level if it is 0.
3. The first 60 samples of the temporary vector are the output. Positions 60 to 63 are dropped.
4. T = (L - 1 + AcLg) + ACELP_PITCH_TAP_OFFSETS[AcGn]. beta = ACELP_PITCH_TAP_GAINS[AcGn]. If T < 58: for n = T to 59 in increasing order, output[n] = output[n] + beta times output[n - T] (in place, recursive, the same as in the encoder, section 8.9 step 7).

Every 12-bit position index and 4-bit sign word is valid, and there is no check.

### 10.4 Invalid indices and erased frames

The reference decoder handles invalid values as follows:

- A pitch gain index (AcGn, section 1.1) that is not below its bound (170 or, for the high rate with L < 58, 85) is detected while the frame is unpacked. The whole frame is then treated as erased, in the same way as a frame flagged erroneous by the channel (this is the decoder's frame erasure concealment of clause 3.10). The four subframes are checked in order after both lag codes have been read; the first failure ends the unpacking. When a frame is erased, none of the fixed codebook decoding of this document is run for it.
- A high rate position index that is not below the codebook size gives a zero fixed vector for that subframe only (section 10.2 step 2).
- The amplitude index MG is a remainder modulo 24, so it is always valid.
- The grid bit and the pulse train bit are single bits and always valid. A pulse train bit of 1 for L >= 58 cannot be received, because bit 11 is then part of the pitch gain index.
- For the low rate nothing can be invalid.

## 11. Adding the fixed and adaptive parts

### 11.1 Decoder

For each subframe i = 0 to 3 of a frame that is not erased (section 10.4), in this order:

1. Decode the fixed excitation vector v[n] (sections 10.2 or 10.3). It is stored in the excitation buffer at the place of the subframe, replacing whatever was there.
2. Compute the adaptive codebook contribution u[n] (clause 2.18, Decod_Acbk), a separate vector, from the excitation buffer contents before this subframe, with L, AcLg and AcGn. The reference reads only samples that precede the current subframe, so the order of steps 1 and 2 does not change the result.
3. e[n] = v[n] + u[n], for n = 0 to 59. It replaces the fixed vector in the excitation buffer.

The sums e[n] are not clipped at this stage. The excitation of the later subframes of the same frame, when they read their adaptive contribution, therefore reads the unclipped values of the earlier subframes of the same frame. When the whole frame is complete, a copy of the 240 samples is made in which every sample below -32767.5 is replaced by -32768.0 and every sample above 32766.5 is replaced by 32767.0 (samples in between are unchanged, they are not rounded). That clipped copy is the excitation memory kept for the next frame and the input of the pitch postfilter filtering. The analysis steps that run before the clipping (the pitch postfilter parameter search and the concealment classifier) read the unclipped frame. When the pitch postfilter is switched off, the synthesis filter receives the unclipped sums. The encoder clips each subframe at once, as the new samples enter its excitation memory (section 12, step 4). The two behaviours differ only when a sum leaves the range of 16-bit numbers.

### 11.2 Pitch-synchronous addition at the low rate

At the low rate the pitch-synchronous contribution is part of the fixed vector (section 10.3 step 4). It is added before the sum of section 11.1 step 3 and is not a separate term.

## 12. Encoder reconstruction after the search, memory update (clause 2.19)

For each subframe i = 0 to 3, after the adaptive search and the fixed search:

1. The adaptive search has replaced the base lag by the final lag of the pair (for even subframes), set AcLg and chosen AcGn. The fixed search (section 3 or 8) has put the fixed excitation vector v[n] in place of the target. At the high rate this is the vector of section 6, expanded by Gen_Trn (section 4) if the pulse train bit is 1. At the low rate this is the vector of section 8.9 step 7.
2. Compute the adaptive contribution u[n] with the same procedure as the decoder (Decod_Acbk), using the final lag, AcLg and AcGn, the five-tap gains of the codebook selected by the rate and the lag, and the excitation memory of the encoder.
3. The total excitation is e[n] = v[n] + u[n], n = 0 to 59.
4. The excitation memory (145 samples) is shifted by 60 samples: element j - 60 receives the old element j, for j = 60 to 144. The new samples are stored in the last 60 places, each clipped as in section 11.1 (below -32767.5 gives -32768.0, above 32766.5 gives 32767.0, otherwise unchanged).
5. The filter memories are updated by passing the unclipped sum e[n] (not the clipped values stored in the excitation memory) through the combined filter (Upd_Ring, clause 2.19). That procedure overwrites e[n] with the output of the filter.

All the quantities of step 2 are those that the decoder will compute from the transmitted parameters, so that the encoder and decoder memories agree. At the high rate the lag used for the pulse train (L) is the final transmitted value, as in the decoder.

## 13. Differences between the text and the reference (collected)

1. Gmax (equation 25): the reference uses the largest magnitude of the correlation per grid, and quantizes by comparing the level times the energy of the impulse response with that magnitude (section 5.3). Ties in the first pulse position go to the later position. Ties in the quantizer go to the larger index, with the starting minimum 32767.0.
2. Candidate gains: indices q* - 2 to q* + 1 (section 5.4), not q* - 1 to q* + 2.
3. All pulses of the high rate have the same magnitude. The sign of each pulse is from the corrected correlation, with zero positive.
4. Pulse train: the train is tried only after the plain search, at L < 58 with the base lag, and wins only by a strictly smaller error. The train period and the limit 58 use the base lag of the pair, also for odd subframes.
5. Sign pre-selection of the low rate: sign of the sum of the pair d[2 j] + d[2 j + 1], not the comparison of equation 32 (section 8.5).
6. Focused search: signed sign-aligned correlations, larger of two thresholds; the 600 limit is a budget that carries over inside the frame (sections 8.5 and 8.7).
7. Pitch-synchronous: limit 58 on T (not 60 on the lag), recursive in-place modification of both the impulse response and the pulse vector.
8. Position index of the low rate: 3 bits per pulse, pulse 0 lowest. Sign bit 1 means positive at the low rate and negative at the high rate.
9. Invalid indices: erasure for the pitch gain index, zero vector for the high rate position index (section 10.4). The text does not describe either.
10. The decoder keeps unclipped sums inside the frame; the encoder clips each subframe immediately (sections 11.1 and 12).
