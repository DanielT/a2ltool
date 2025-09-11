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
        let mut previous_single_line_comment = false;
        let mut pos = 0;

        // Iterate through the data to find comments; if there are less than two bytes left, there is nothing useful to do.
        while pos < data.len() - 2 {
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
                    previous_single_line_comment = false;
                }
                b'\'' => {
                    pos = skip_char_literal(data, find_pos);
                    previous_single_line_comment = false;
                }
                b'/' => {
                    if data[find_pos + 1] == b'/' {
                        let should_merge = previous_single_line_comment
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
                        }

                        previous_single_line_comment = true; // Set comment state
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
                        previous_single_line_comment = false;
                    } else {
                        // Not a comment, just a single slash, continue processing
                        pos += 1;
                        previous_single_line_comment = false;
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
                    let end = memchr(b'"', &remaining[1..]).unwrap_or(remaining.len()) + 1;
                    parts.push(&remaining[..end + 1]); // this includes the closing quote
                    remaining = &remaining[end + 1..];
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
