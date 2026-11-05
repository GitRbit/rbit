//! Diagnostic expansion affects catalog literals, never repository identifiers.

pub(crate) fn expand(output: &mut String, text: &str) {
    for character in text.chars() {
        output.push(match character {
            'a' => 'á',
            'A' => 'Á',
            'e' => 'ë',
            'E' => 'Ë',
            'i' => 'ï',
            'I' => 'Ï',
            'o' => 'ô',
            'O' => 'Ô',
            'u' => 'ü',
            'U' => 'Ü',
            'c' => 'ç',
            'C' => 'Ç',
            'n' => 'ñ',
            'N' => 'Ñ',
            _ => character,
        });
        if matches!(character, 'a' | 'e' | 'i' | 'o' | 'u' | 'A' | 'E' | 'I' | 'O' | 'U') {
            output.push('~');
        }
    }
}
