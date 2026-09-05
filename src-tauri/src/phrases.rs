//! Recover well-known dictation phrases when STT mangles them.

pub const NALIM_TONGUE_TWISTER: &str = "На мели мы лениво налима ловили, И меняли налима вы мне на линя, О любви не меня ли вы мило молили И в туманы лимана манили меня";

pub const SASHA_TONGUE_TWISTER: &str = "Шла Саша по шоссе и сосала сушку";

struct KnownPhrase {
    canonical: &'static str,
    stems: &'static [&'static str],
    min_stems: usize,
    min_score: f64,
}

fn known() -> [KnownPhrase; 2] {
    [
        KnownPhrase {
            canonical: NALIM_TONGUE_TWISTER,
            stems: &[
                "налим",
                "лиман",
                "ленив",
                "ловил",
                "мели",
                "линя",
                "любв",
                "молил",
                "манил",
                "туман",
            ],
            min_stems: 5,
            min_score: 0.55,
        },
        KnownPhrase {
            canonical: SASHA_TONGUE_TWISTER,
            stems: &["саша", "шосс", "сос", "сушк", "шла"],
            min_stems: 3,
            min_score: 0.58,
        },
    ]
}

pub fn recover(text: &str) -> String {
    let compact = compact_alpha(text);
    for phrase in known() {
        let target = compact_alpha(phrase.canonical);
        let heard_len = compact.chars().count();
        let target_len = target.chars().count();
        if target_len == 0 || heard_len < (target_len * 55 / 100).max(12) {
            continue;
        }
        let ratio = heard_len as f64 / target_len as f64;
        if ratio < 0.55 || ratio > 1.7 {
            continue;
        }
        let stems = phrase
            .stems
            .iter()
            .filter(|stem| compact.contains(*stem))
            .count();
        let score = similarity(&compact, &target);
        if score >= phrase.min_score
            || (stems >= phrase.min_stems && score >= (phrase.min_score - 0.12).max(0.45))
        {
            return phrase.canonical.to_string();
        }
    }
    text.to_string()
}

fn compact_alpha(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'ё' | 'Ё' => 'е',
            'і' | 'І' | 'ї' | 'Ї' => 'и',
            other => other,
        })
        .filter(|c| c.is_alphabetic())
        .flat_map(char::to_lowercase)
        .collect()
}

fn similarity(left: &str, right: &str) -> f64 {
    let left_len = left.chars().count();
    let right_len = right.chars().count();
    let max = left_len.max(right_len);
    if max == 0 {
        return 1.0;
    }
    1.0 - (levenshtein(left, right) as f64 / max as f64)
}

fn levenshtein(left: &str, right: &str) -> usize {
    let a: Vec<char> = left.chars().collect();
    let b: Vec<char> = right.chars().collect();
    if a.len() > 400 || b.len() > 400 {
        return a.len().max(b.len());
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovers_mangled_nalim_tongue_twister() {
        let heard = "На милимы лениваловили налимо, на милимы лениваловили ления, а любви не. Не меняли вы мило молили, и в туману лимана молили меня.";
        assert_eq!(recover(heard), NALIM_TONGUE_TWISTER);
    }

    #[test]
    fn recovers_mangled_sasha_tongue_twister() {
        assert_eq!(
            recover("Шла саша паше си і сасала сушку."),
            SASHA_TONGUE_TWISTER
        );
        assert_eq!(
            recover("шла саша по шоссе и сосала сушку"),
            SASHA_TONGUE_TWISTER
        );
    }

    #[test]
    fn leaves_ordinary_speech_alone() {
        assert_eq!(recover("Привет, как дела?"), "Привет, как дела?");
        assert_eq!(
            recover("Мы вчера ловили налима на мели."),
            "Мы вчера ловили налима на мели."
        );
        assert_eq!(recover("Привет, Саша"), "Привет, Саша");
    }

    #[test]
    fn compact_alpha_maps_ukrainian_i() {
        assert_eq!(
            recover("шла саша по шоссе і сосала сушку"),
            SASHA_TONGUE_TWISTER
        );
    }

    #[test]
    fn empty_and_short_text_pass_through() {
        assert_eq!(recover(""), "");
        assert_eq!(recover("ок"), "ок");
        assert_eq!(recover("налим"), "налим");
    }
}
