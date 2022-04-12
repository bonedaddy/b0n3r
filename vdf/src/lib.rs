/// Modular Square Roots-based Verifiable Delay Function (VDF) implementation.
pub mod vdf_mod_sqrt {
    use rug::Integer;

    /// Verifies that delay function from given `seed` was calculated and produced a `witness`
    pub fn verify(modulus: &Integer, seed: &Integer, num_steps: u64, witness: &Integer) -> bool {
        // Get instance of 2 in Integer format for performing of squares
        let square: Integer = 2u64.into();

        // Perform NUM_ITERS of sequential modular squares to perform a verification of the solution
        let mut result = witness.clone();
        for _ in 0..num_steps {
            // Perform a simple and fast modular squaring
            result.pow_mod_mut(&square, &modulus).unwrap();

            // Perform an iterating permutation in Fp
            let inv_result = -result;
            let r = inv_result.div_rem_floor_ref(&modulus);
            result = <(Integer, Integer)>::from(r).1;

            // Perform XOR of the result as a basic secure permutation
            // against possible modular square root short circuits
            result ^= 1;
            while result >= *modulus || result == 0 {
                // XOR in such a way that result won't exceed modulus
                result ^= 1;
            }
        }

        result == seed.clone().div_rem_floor(modulus.clone()).1
    }

    /// A verifiable delay function based on the
    /// slow sequential function based on permutations in Fp (sequential modulo square roots)
    /// It should be slow (or at least non-parallelizable) to compute but (very) fast to verify.
    pub fn eval(modulus: &Integer, seed: &Integer, num_steps: u64) -> Integer {
        // Allocate our own exponentiation moduli
        let modulus = modulus.clone();

        // Take seed by moduli p
        let mut x = Integer::from(seed.clone().div_rem_floor(modulus.clone()).1);

        // Exponent for square root calculation
        let exponent = (modulus.clone() + 1) / 4;

        // Perform `NUM_ITERS` sequential modular square root computations
        for _ in 0..num_steps {
            // Perform XOR of the result as a basic secure permutation
            // against possible modular square root short circuits
            x ^= 1;
            while x >= modulus || x == 0 {
                // XOR in such a way that result won't exceed modulus
                x ^= 1;
            }

            // Perform a slow modular square root extraction
            x.pow_mod_mut(&exponent, &modulus).unwrap();
        }

        x
    }
}

/// MiMC[1]-based VDF (see Vitalik Buterin's article that applies MiMC as VDF [2])
///
/// [1]: https://eprint.iacr.org/2016/492.pdf
/// [2]: https://vitalik.ca/general/2018/07/21/starks_part_3.html
pub mod vdf_mimc {
    use rug::Integer;

    /// Modulus of prime field 2^256 - 2^32 * 351 + 1
    pub const MODULUS: &str =
        "115792089237316195423570985008687907853269984665640564039457584006405596119041";

    /// An exponent to perform inverse of x^3 on prime field based on Fermat's Little Theorem
    pub const L_FERMAT_EXPONENT: &str =
        "77194726158210796949047323339125271902179989777093709359638389337603730746027";

    /// Calculates set of round constants to perform MiMC-calculation on.
    fn calculate_round_constants() -> [u64; 64] {
        let mut round_constants = [0u64; 64];
        for i in 0usize..64 {
            round_constants[i] = (i.pow(7) ^ 42) as u64;
        }

        round_constants
    }

    /// Executes `num_steps` of MiMC-calculation in forward direction for the given `input`
    fn forward_mimc(num_steps: u64, input: &Integer) -> Integer {
        let modulus = Integer::from_str_radix(MODULUS, 10).unwrap();
        let round_constants = calculate_round_constants();

        let mut result = input.clone();
        let three = Integer::from(3);
        for i in 1..num_steps {
            result = (result.pow_mod(&three, &modulus).unwrap()
                + Integer::from(round_constants[i as usize % round_constants.len()]))
                % &modulus;
        }

        result
    }

    /// Executes `num_steps` of MiMC-calculation in backward direction for the given `input`.
    ///
    /// The properties of MiMC-scheme guarantees that calculation in backward direction is
    /// always slower than in forward for correctly chosen parameters.
    fn backward_mimc(num_steps: u64, input: &Integer) -> Integer {
        let modulus = Integer::from_str_radix(MODULUS, 10).unwrap();
        let l_fermat_exp = Integer::from_str_radix(L_FERMAT_EXPONENT, 10).unwrap();
        let round_constants = calculate_round_constants();

        let mut result = input.clone();
        for i in (1..num_steps).rev() {
            let round_constant = Integer::from(round_constants[i as usize % round_constants.len()]);
            result = Integer::from(&result - &round_constant)
                .pow_mod(&l_fermat_exp, &modulus)
                .unwrap();
        }

        result
    }

    /// Performs an Eval() step of the MiMC-based VDF
    pub fn eval(seed: &Integer, num_steps: u64) -> Integer {
        let witness = backward_mimc(num_steps, &seed);

        witness
    }

    /// Performs a Verify() step for the MiMC-based VDF result
    pub fn verify(seed: &Integer, num_steps: u64, witness: &Integer) -> bool {
        let result = forward_mimc(num_steps, witness);

        result == *seed
    }

    #[cfg(test)]
    mod tests {
        use std::time;

        use super::*;

        use rug::Integer;

        #[test]
        fn test() {
            const NUM_STEPS: u64 = 1024 * 512; // Approx. 1 min. on MBP 13"
            let seed = rug::Integer::from_f64(1_f64).unwrap();

            println!("Calculating MiMC VDF for input: {}", seed);
            let eval_start = time::SystemTime::now();
            let result = eval(&seed, NUM_STEPS);
            let eval_duration = eval_start.elapsed().unwrap();
            println!("result {}. elapsed {}", result, eval_duration.as_secs());
            println!("Calculating forward MiMC:");

            let verify_start = time::SystemTime::now();
            let valid = verify(&seed, NUM_STEPS, &result);
            let verify_duration = verify_start.elapsed().unwrap();
            if !valid {
                panic!("should be valid");
            }
            println!("verify duration {}", verify_duration.as_secs());
        }
    }
}
