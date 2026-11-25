// Rust reads these limits at compile time and C++ includes the file, so it holds `static const int` lines only.

// Shadow cascades, one light_space matrix each.
static const int CASCADE_COUNT = 3;
// Past 5 a step moves under 2 % of near-shadow pixels while taps grow as (2r+1)^2.
static const int MAX_PCF_RADIUS = 5;
