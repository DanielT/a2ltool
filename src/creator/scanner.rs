use memchr::memchr;

type PreprocessedInput<'a> = Vec<&'a [u8]>;

pub(crate) struct CommentScanner<'prefix> {
    comment_prefix: &'prefix [u8],
    block_end_finder: memchr::memmem::Finder<'static>,
    range_step_finder: memchr::memmem::Finder<'static>,
}

impl<'prefix> CommentScanner<'prefix> {
    pub(crate) fn new(comment_prefix: &'prefix [u8]) -> Self {
        let block_end_finder = memchr::memmem::Finder::new(b"*/");
        let range_step_finder = memchr::memmem::Finder::new(b"],");

        CommentScanner {
            comment_prefix,
            block_end_finder,
            range_step_finder,
        }
    }

    /// Find all comments in the source code.
    pub(crate) fn scan_comments<'a>(&self, data: &'a [u8]) -> Vec<(usize, PreprocessedInput<'a>)> {
        let mut commands: Vec<(usize, PreprocessedInput)> = vec![];
        // Track whether the previous single-line comment contained a command, to determine if we should
        // merge consecutive single-line command comments. This is done if they are separated only by whitespace.
        let mut previous_line_had_command = false;
        let mut pos = 0;

        // Iterate through the data to find comments; if there are less than two bytes left, there is nothing useful to do.
        while pos + 1 < data.len() {
            /*
            Process the source code to find comments. The naive approach of only looking for a leading slash will not work:
            - slashes can be stored in strings
            - slashes can be enclosed in character literals
            Recognizing strings has essentially the same problems:
            - the leading quote can be inside a comment
            - the leading quote can be enclosed in a character literal
            This makes it necessary to maintain some state about the current context, tracking open strings and comments.
            */

            let Some(rel_pos) = memchr::memchr3(b'/', b'"', b'\'', &data[pos..]) else {
                // none found
                break;
            };

            let find_pos = pos + rel_pos;
            match data[find_pos] {
                b'"' => {
                    pos = skip_string_literal(data, find_pos);
                    previous_line_had_command = false;
                }
                b'\'' => {
                    pos = skip_char_literal(data, find_pos);
                    previous_line_had_command = false;
                }
                b'/' => {
                    if find_pos + 1 >= data.len() {
                        // a '/' as the last byte of the data cannot start a comment
                        break;
                    }
                    if data[find_pos + 1] == b'/' {
                        let should_merge = previous_line_had_command
                            && data[pos..find_pos].iter().all(|&b| b.is_ascii_whitespace());

                        if let Some(mut cur_command) =
                            self.handle_line_comment(data, &mut pos, find_pos)
                            && !cur_command.is_empty()
                        {
                            if should_merge && let Some(last_command) = commands.last_mut() {
                                // Merge with the previous single-line comment
                                last_command.1.append(&mut cur_command);
                            } else {
                                commands.push((find_pos, cur_command));
                            }
                            // only a comment line that contained a command can be merged with the next line;
                            // an ordinary comment ends any command sequence
                            previous_line_had_command = true;
                        } else {
                            previous_line_had_command = false;
                        }
                    } else if data[find_pos + 1] == b'*' {
                        // Multi-line comment, find the closing '*/'
                        if let Some(end_comment_rel_pos) =
                            self.block_end_finder.find(&data[find_pos + 2..])
                        {
                            let comment = &data[find_pos + 2..find_pos + 2 + end_comment_rel_pos];

                            let cmd = comment
                                .split(|&b| b == b'\n')
                                .filter_map(|line| self.split_command(line))
                                .flatten()
                                .collect::<Vec<_>>();
                            if !cmd.is_empty() {
                                commands.push((find_pos, cmd));
                            }

                            pos = find_pos + 2 + end_comment_rel_pos + 2; // Move past the '*/'
                        } else {
                            // No closing '*/' found, ignore the rest of the data
                            pos = data.len();
                        }
                        previous_line_had_command = false;
                    } else {
                        // Not a comment, just a single slash, continue processing
                        pos += 1;
                        previous_line_had_command = false;
                    }
                }
                _ => {
                    // This case should not happen, as we only search for '/', '"', and '\''.
                    unreachable!();
                }
            }
        }

        commands
    }

    fn handle_line_comment<'a>(
        &self,
        data: &'a [u8],
        pos: &mut usize,
        find_pos: usize,
    ) -> Option<Vec<&'a [u8]>> {
        // Single-line comment
        let cur_comment =
            if let Some(newline_rel_pos) = memchr::memchr(b'\n', &data[find_pos + 2..]) {
                let comment = &data[find_pos + 2..=find_pos + 2 + newline_rel_pos]; // Include the newline
                *pos = find_pos + 2 + newline_rel_pos + 1; // Move past the newline
                comment
            } else {
                let comment = &data[find_pos + 2..];
                // No newline found
                *pos = data.len(); // Move to the end of the data
                comment
            };

        self.split_command(cur_comment)
    }

    fn split_command<'a>(&self, comment_line: &'a [u8]) -> Option<Vec<&'a [u8]>> {
        if let Some(creator_cmd) = comment_line
            .trim_ascii_start()
            .strip_prefix(self.comment_prefix)
        {
            let mut parts = vec![];
            let mut remaining = creator_cmd;
            while !remaining.is_empty() {
                remaining = remaining.trim_ascii_start();
                if remaining.is_empty() {
                    break;
                }
                if remaining[0] == b'"' {
                    // whole strings may include whitespace, e.g in descriptions
                    // find the closing double quote
                    if let Some(rel_end) = memchr(b'"', &remaining[1..]) {
                        let end = rel_end + 2; // this includes the closing quote
                        parts.push(&remaining[..end]);
                        remaining = &remaining[end..];
                    } else {
                        // unterminated string: the token has no closing quote, so the parser will reject it
                        parts.push(remaining.trim_ascii_end());
                        remaining = &[];
                    }
                } else if remaining[0] == b'=' {
                    // equals sign should be a separate token, even if it is not separated with whitespace
                    parts.push(&remaining[..1]);
                    remaining = &remaining[1..];
                } else if remaining[0] == b'[' {
                    // the opening bracket of a range should be present as a separate token.
                    // It is not allowed as the first character inside a symbol name, so the only case where it occurs on the first position is as part of range notation
                    parts.push(&remaining[..1]);
                    remaining = &remaining[1..];
                } else {
                    // all other tokens are space-separated. There are too many different kinds of whitespace to use memchr
                    // additionally, '=' can be a separator.
                    let end = remaining
                        .iter()
                        .position(|c| c.is_ascii_whitespace() || *c == b'=')
                        .unwrap_or(remaining.len());
                    let token = &remaining[..end];

                    if let Some(pos) = self.range_step_finder.find(token) {
                        // special case for the range + step notation "],"
                        if pos > 0 {
                            // range value before the closing bracket
                            parts.push(&token[..pos]);
                        }
                        parts.push(&token[pos..pos + 1]); // closing bracket
                        parts.push(&token[pos + 1..pos + 2]); // comma
                        if pos + 2 < token.len() {
                            // step value after the comma
                            parts.push(&token[pos + 2..]);
                        }
                    } else {
                        parts.push(token);
                    }

                    remaining = &remaining[end..];
                }
            }
            Some(parts)
        } else {
            None
        }
    }
}

fn skip_char_literal(data: &[u8], pos: usize) -> usize {
    let data_len = data.len();
    // we already know at this point that the first byte is a single quote
    if data_len > pos + 2 && data[pos + 2] == b'\'' {
        pos + 3 // Single character literal, skip to the end
    } else if data_len > pos + 4 && data[pos + 1] == b'\\' && data[pos + 3] == b'\'' {
        pos + 4 // Escaped character literal, skip to the end
    } else {
        pos + 1 // Not a valid character literal, just move one byte forward
    }
}

fn skip_string_literal(data: &[u8], pos: usize) -> usize {
    let mut pos = pos + 1;
    while pos < data.len() {
        if data[pos] == b'\\' {
            pos += 2; // Skip escaped character
        } else if data[pos] == b'"' {
            pos += 1; // Move past the closing quote
            break;
        } else {
            pos += 1;
        }
    }
    pos
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skip_string_literal() {
        let data = b"\"Hello, \\\"world\\\"!\"";
        let pos = skip_string_literal(data, 0);
        assert_eq!(pos, data.len());
    }

    #[test]
    fn test_skip_char_literal() {
        let data = b"'a'";
        let pos = skip_char_literal(data, 0);
        assert_eq!(pos, data.len());
    }

    #[test]
    fn malformed_input() {
        let scanner = CommentScanner::new(b"@@ ");

        // very short inputs and inputs ending in '/' must not panic
        assert!(scanner.scan_comments(b"").is_empty());
        assert!(scanner.scan_comments(b"/").is_empty());
        assert!(scanner.scan_comments(b"ab").is_empty());
        assert!(scanner.scan_comments(b"abc/").is_empty());

        // an unterminated string in a creator comment must not panic;
        // the rest of the line becomes a single token without a closing quote
        let comments = scanner.scan_comments(b"// @@ DESCRIPTION = \"missing close quote\n");
        assert_eq!(comments.len(), 1);
        let (_, tokens) = &comments[0];
        assert_eq!(tokens[2], b"\"missing close quote");

        // a lone '"' at the end of a comment must not panic either
        let comments = scanner.scan_comments(b"// @@ DESCRIPTION = \"\n");
        assert_eq!(comments.len(), 1);
        let (_, tokens) = &comments[0];
        assert_eq!(tokens[2], b"\"");
    }

    #[test]
    fn ordinary_comment_ends_command_sequence() {
        let input = br#"
        // @@ SYMBOL = a
        // @@ END
        // an ordinary comment, not a command
        // @@ SYMBOL = b
        // @@ END
        "#;
        let scanner = CommentScanner::new(b"@@");
        let comments = scanner.scan_comments(input);
        // The ordinary comment interrupts the merging of single-line comments, so two
        // separate commands result. Previously the command after the ordinary comment
        // was appended to the first command.
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].1, vec![b"SYMBOL" as &[u8], b"=", b"a", b"END"]);
        assert_eq!(comments[1].1, vec![b"SYMBOL" as &[u8], b"=", b"b", b"END"]);
    }

    #[test]
    fn comment_scanner() {
        let input = br#"
        abc
        "def\""
        // regular comment
        ---
        // @@ looks like a definition
        struct whatever {
            int thing;
        };
        /*
        @@ defintion block with various specific cases
        @@ compact range = [0...1.0],555
        @@ alternative range = [ 0 ... 1.0 ], 555
        */
        y = x / 3;
        "#;

        let scanner = CommentScanner::new(b"@@ ");
        let comments = scanner.scan_comments(input);
        assert_eq!(comments.len(), 2);
        let (_, first_comment) = &comments[0];
        assert_eq!(first_comment[0], b"looks");
        assert_eq!(first_comment[1], b"like");
        assert_eq!(first_comment[2], b"a");
        assert_eq!(first_comment[3], b"definition");
    }
}
