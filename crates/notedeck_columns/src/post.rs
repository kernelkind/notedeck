use egui::TextBuffer;
use enostr::{FullKeypair, Pubkey};
use nostrdb::{Note, NoteBuilder, NoteReply};
use std::{
    collections::{BTreeMap, HashSet},
    ops::Range,
};
use tracing::error;

use crate::media_upload::Nip94Event;

pub struct NewPost {
    pub content: String,
    pub account: FullKeypair,
    pub media: Vec<Nip94Event>,
    pub mentions: Vec<Pubkey>,
}

fn add_client_tag(builder: NoteBuilder<'_>) -> NoteBuilder<'_> {
    builder
        .start_tag()
        .tag_str("client")
        .tag_str("Damus Notedeck")
}

impl NewPost {
    pub fn new(
        content: String,
        account: enostr::FullKeypair,
        media: Vec<Nip94Event>,
        mentions: Vec<Pubkey>,
    ) -> Self {
        NewPost {
            content,
            account,
            media,
            mentions,
        }
    }

    pub fn to_note(&self, seckey: &[u8; 32]) -> Note {
        let mut content = self.content.clone();
        append_urls(&mut content, &self.media);

        let mut builder = add_client_tag(NoteBuilder::new()).kind(1).content(&content);

        for hashtag in Self::extract_hashtags(&self.content) {
            builder = builder.start_tag().tag_str("t").tag_str(&hashtag);
        }

        if !self.media.is_empty() {
            builder = add_imeta_tags(builder, &self.media);
        }

        if !self.mentions.is_empty() {
            builder = add_mention_tags(builder, &self.mentions);
        }

        builder.sign(seckey).build().expect("note should be ok")
    }

    pub fn to_reply(&self, seckey: &[u8; 32], replying_to: &Note) -> Note {
        let mut content = self.content.clone();
        append_urls(&mut content, &self.media);

        let builder = add_client_tag(NoteBuilder::new()).kind(1).content(&content);

        let nip10 = NoteReply::new(replying_to.tags());

        let mut builder = if let Some(root) = nip10.root() {
            builder
                .start_tag()
                .tag_str("e")
                .tag_str(&hex::encode(root.id))
                .tag_str("")
                .tag_str("root")
                .start_tag()
                .tag_str("e")
                .tag_str(&hex::encode(replying_to.id()))
                .tag_str("")
                .tag_str("reply")
                .sign(seckey)
        } else {
            // we're replying to a post that isn't in a thread,
            // just add a single reply-to-root tag
            builder
                .start_tag()
                .tag_str("e")
                .tag_str(&hex::encode(replying_to.id()))
                .tag_str("")
                .tag_str("root")
                .sign(seckey)
        };

        let mut seen_p: HashSet<&[u8; 32]> = HashSet::new();

        builder = builder
            .start_tag()
            .tag_str("p")
            .tag_str(&hex::encode(replying_to.pubkey()));

        seen_p.insert(replying_to.pubkey());

        for tag in replying_to.tags() {
            if tag.count() < 2 {
                continue;
            }

            if tag.get_unchecked(0).variant().str() != Some("p") {
                continue;
            }

            let id = if let Some(id) = tag.get_unchecked(1).variant().id() {
                id
            } else {
                continue;
            };

            if seen_p.contains(id) {
                continue;
            }

            seen_p.insert(id);

            builder = builder.start_tag().tag_str("p").tag_str(&hex::encode(id));
        }

        if !self.media.is_empty() {
            builder = add_imeta_tags(builder, &self.media);
        }

        if !self.mentions.is_empty() {
            builder = add_mention_tags(builder, &self.mentions);
        }

        builder
            .sign(seckey)
            .build()
            .expect("expected build to work")
    }

    pub fn to_quote(&self, seckey: &[u8; 32], quoting: &Note) -> Note {
        let mut new_content = format!(
            "{}\nnostr:{}",
            self.content,
            enostr::NoteId::new(*quoting.id()).to_bech().unwrap()
        );

        append_urls(&mut new_content, &self.media);

        let mut builder = NoteBuilder::new().kind(1).content(&new_content);

        for hashtag in Self::extract_hashtags(&self.content) {
            builder = builder.start_tag().tag_str("t").tag_str(&hashtag);
        }

        if !self.media.is_empty() {
            builder = add_imeta_tags(builder, &self.media);
        }

        if !self.mentions.is_empty() {
            builder = add_mention_tags(builder, &self.mentions);
        }

        builder
            .start_tag()
            .tag_str("q")
            .tag_str(&hex::encode(quoting.id()))
            .start_tag()
            .tag_str("p")
            .tag_str(&hex::encode(quoting.pubkey()))
            .sign(seckey)
            .build()
            .expect("expected build to work")
    }

    fn extract_hashtags(content: &str) -> HashSet<String> {
        let mut hashtags = HashSet::new();
        for word in
            content.split(|c: char| c.is_whitespace() || (c.is_ascii_punctuation() && c != '#'))
        {
            if word.starts_with('#') && word.len() > 1 {
                let tag = word[1..].to_lowercase();
                if !tag.is_empty() {
                    hashtags.insert(tag);
                }
            }
        }
        hashtags
    }
}

fn append_urls(content: &mut String, media: &Vec<Nip94Event>) {
    for ev in media {
        content.push(' ');
        content.push_str(&ev.url);
    }
}

fn add_mention_tags<'a>(builder: NoteBuilder<'a>, mentions: &Vec<Pubkey>) -> NoteBuilder<'a> {
    let mut builder = builder;

    for mention in mentions {
        builder = builder.start_tag().tag_str("p").tag_str(&mention.hex());
    }

    builder
}

fn add_imeta_tags<'a>(builder: NoteBuilder<'a>, media: &Vec<Nip94Event>) -> NoteBuilder<'a> {
    let mut builder = builder;
    for item in media {
        builder = builder
            .start_tag()
            .tag_str("imeta")
            .tag_str(&format!("url {}", item.url));

        if let Some(ox) = &item.ox {
            builder = builder.tag_str(&format!("ox {ox}"));
        };
        if let Some(x) = &item.x {
            builder = builder.tag_str(&format!("x {x}"));
        }
        if let Some(media_type) = &item.media_type {
            builder = builder.tag_str(&format!("m {media_type}"));
        }
        if let Some(dims) = &item.dimensions {
            builder = builder.tag_str(&format!("dim {}x{}", dims.0, dims.1));
        }
        if let Some(bh) = &item.blurhash {
            builder = builder.tag_str(&format!("blurhash {bh}"));
        }
        if let Some(thumb) = &item.thumb {
            builder = builder.tag_str(&format!("thumb {thumb}"));
        }
    }
    builder
}

#[derive(Debug, Clone)]
pub struct PostBuffer {
    pub text_buffer: String,
    pub mention_indicator: char,
    pub mentions: Vec<MentionInfo>,

    // the start index of a mention is inclusive
    pub mention_starts: BTreeMap<usize, usize>, // maps the mention start index with the `Self::mentions` Vec

    // the end index of a mention is exclusive
    pub mention_ends: BTreeMap<usize, usize>, // maps the mention end index with the `Self::mentions` Vec
}

impl Default for PostBuffer {
    fn default() -> Self {
        Self {
            mention_indicator: '@',
            text_buffer: Default::default(),
            mentions: Default::default(),
            mention_starts: Default::default(),
            mention_ends: Default::default(),
        }
    }
}

impl PostBuffer {
    pub fn get_mention(&self, cursor_index: usize) -> Option<MentionIndex<'_>> {
        self.mention_ends
            .range(cursor_index..)
            .next()
            .and_then(|(_, mention_index)| {
                self.mentions
                    .get(*mention_index)
                    .filter(|info| {
                        if let MentionType::Finalized(_) = info.mention_type {
                            // should exclude the last character if we're finalized
                            info.start_index <= cursor_index && cursor_index < info.end_index
                        } else {
                            info.start_index <= cursor_index && cursor_index <= info.end_index
                        }
                    })
                    .map(|info| MentionIndex {
                        index: *mention_index,
                        info,
                    })
            })
    }

    pub fn get_mention_string<'a>(&'a self, mention_index: &MentionIndex<'a>) -> &'a str {
        self.text_buffer
            .char_range(mention_index.info.start_index + 1..mention_index.info.end_index)
        // don't include the delim
    }

    pub fn select_full_mention(&mut self, mention_index: usize, pk: Pubkey) {
        if let Some(info) = self.mentions.get_mut(mention_index) {
            info.mention_type = MentionType::Finalized(pk);
        } else {
            error!("Error selecting mention for index: {mention_index}. Have the following mentions: {:?}", self.mentions);
        }
    }

    pub fn select_mention_and_replace_name(
        &mut self,
        mention_index: usize,
        full_name: &str,
        pk: Pubkey,
    ) {
        if let Some(info) = self.mentions.get(mention_index) {
            let text_start_index = info.start_index + 1;
            self.delete_char_range(text_start_index..info.end_index);
            self.insert_text(full_name, text_start_index);
            self.select_full_mention(mention_index, pk);
        } else {
            error!("Error selecting mention for index: {mention_index}. Have the following mentions: {:?}", self.mentions);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.text_buffer.is_empty()
    }

    pub fn output(&self) -> PostOutput {
        let mut out = self.text_buffer.clone();
        let mut mentions = Vec::new();
        for (cur_end_ind, mention_ind) in self.mention_ends.iter().rev() {
            if let Some(info) = self.mentions.get(*mention_ind) {
                if let MentionType::Finalized(pk) = info.mention_type {
                    if let Some(bech) = pk.to_bech() {
                        out.replace_range(info.start_index..*cur_end_ind, &format!("nostr:{bech}"));
                        mentions.push(pk);
                    }
                }
            }
        }
        mentions.reverse();

        PostOutput {
            text: out,
            mentions,
        }
    }
}

pub struct PostOutput {
    pub text: String,
    pub mentions: Vec<Pubkey>,
}

pub struct MentionIndex<'a> {
    pub index: usize,
    pub info: &'a MentionInfo,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MentionType {
    Pending,
    Finalized(Pubkey),
}

impl TextBuffer for PostBuffer {
    fn is_mutable(&self) -> bool {
        true
    }

    fn as_str(&self) -> &str {
        self.text_buffer.as_str()
    }

    fn insert_text(&mut self, text: &str, char_index: usize) -> usize {
        if text.is_empty() {
            return 0;
        }
        let text_len = text.len();
        self.text_buffer.insert_text(text, char_index);

        // the text was inserted before or inside these mentions. We need to at least move their ends
        let pending_ends_to_update: Vec<usize> = self
            .mention_ends
            .range(char_index..)
            .filter(|(k, v)| {
                let is_last = **k == char_index;
                let is_finalized = if let Some(info) = self.mentions.get(**v) {
                    matches!(info.mention_type, MentionType::Finalized(_))
                } else {
                    false
                };
                !(is_last && is_finalized)
            })
            .map(|(&k, _)| k)
            .collect();

        for cur_end in pending_ends_to_update {
            let mention_index = if let Some(mention_index) = self.mention_ends.get(&cur_end) {
                *mention_index
            } else {
                continue;
            };

            self.mention_ends.remove(&cur_end);

            let new_end = cur_end + text_len;
            self.mention_ends.insert(new_end, mention_index);
            // replaced the current end with the new value

            if let Some(mention_info) = self.mentions.get_mut(mention_index) {
                if mention_info.start_index >= char_index {
                    // the text is being inserted before this mention. move the start index as well
                    self.mention_starts.remove(&mention_info.start_index);
                    let new_start = mention_info.start_index + text_len;
                    self.mention_starts.insert(new_start, mention_index);
                    mention_info.start_index = new_start;
                } else {
                    // text is being inserted inside this mention. Make sure it is in the pending state
                    mention_info.mention_type = MentionType::Pending;
                }

                mention_info.end_index = new_end;
            } else {
                error!("Could not find mention at index {}", mention_index);
            }
        }

        // Begin mention if the inserted character is the mention indicator
        if let Some(char) = text.chars().next() {
            if char == self.mention_indicator {
                let mention_index = self.mentions.len();
                let start_index = char_index;
                let end_index = char_index + text.len();
                self.mentions.push(MentionInfo {
                    start_index,
                    end_index,
                    mention_type: MentionType::Pending,
                });
                self.mention_starts.insert(start_index, mention_index);
                self.mention_ends.insert(end_index, mention_index);
            }
        }

        text_len
    }

    fn delete_char_range(&mut self, char_range: Range<usize>) {
        let Range { start, end } = char_range;
        let char_count = end - start;
        if char_count == 0 {
            return;
        }

        self.text_buffer.drain(start..end);

        // these mentions will be affected by the deletion
        let ends_to_update: Vec<usize> =
            self.mention_ends.range(start..).map(|(&k, _)| k).collect();

        for cur_end in ends_to_update {
            if let Some(mention_index) = self.mention_ends.remove(&cur_end) {
                if let Some(mention_info) = self.mentions.get_mut(mention_index) {
                    // If mention is fully within the deleted range, remove it
                    if mention_info.start_index >= start && mention_info.end_index <= end {
                        self.mention_starts.remove(&mention_info.start_index);
                        self.mentions.remove(mention_index);
                        continue;
                    }

                    // Check if only part of the mention is deleted
                    let is_partially_deleted = (mention_info.start_index < end
                        && mention_info.end_index > end)
                        || (mention_info.start_index < start && mention_info.end_index > start);

                    if is_partially_deleted {
                        // Convert back to Pending if it's Finalized
                        if let MentionType::Finalized(_) = mention_info.mention_type {
                            mention_info.mention_type = MentionType::Pending;
                        }
                    }

                    // Adjust start index if necessary
                    if mention_info.start_index >= end {
                        self.mention_starts.remove(&mention_info.start_index);
                        mention_info.start_index -= char_count;
                        self.mention_starts
                            .insert(mention_info.start_index, mention_index);
                    }

                    // Adjust end index
                    mention_info.end_index -= char_count;
                    self.mention_ends
                        .insert(mention_info.end_index, mention_index);
                } else {
                    error!("Could not find mention at index {}", mention_index);
                }
            }
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct MentionInfo {
    pub start_index: usize,
    pub end_index: usize,
    pub mention_type: MentionType,
}

#[cfg(test)]
mod tests {
    use super::*;
    impl MentionInfo {
        pub fn bounds(&self) -> (usize, usize) {
            (self.start_index, self.end_index)
        }
    }

    const JB55: fn() -> Pubkey = || {
        Pubkey::from_hex("32e1827635450ebb3c5a7d12c1f8e7b2b514439ac10a67eef3d9fd9c5c68e245")
            .unwrap()
    };
    const KK: fn() -> Pubkey = || {
        Pubkey::from_hex("4a0510f26880d40e432f4865cb5714d9d3c200ca6ebb16b418ae6c555f574967")
            .unwrap()
    };

    #[test]
    fn test_extract_hashtags() {
        let test_cases = vec![
            ("Hello #world", vec!["world"]),
            ("Multiple #tags #in #one post", vec!["tags", "in", "one"]),
            ("No hashtags here", vec![]),
            ("#tag1 with #tag2!", vec!["tag1", "tag2"]),
            ("Ignore # empty", vec![]),
            ("Testing emoji #🍌banana", vec!["🍌banana"]),
            ("Testing emoji #🍌", vec!["🍌"]),
            ("Duplicate #tag #tag #tag", vec!["tag"]),
            ("Mixed case #TaG #tag #TAG", vec!["tag"]),
            (
                "#tag1, #tag2, #tag3 with commas",
                vec!["tag1", "tag2", "tag3"],
            ),
            ("Separated by commas #tag1,#tag2", vec!["tag1", "tag2"]),
            ("Separated by periods #tag1.#tag2", vec!["tag1", "tag2"]),
            ("Separated by semicolons #tag1;#tag2", vec!["tag1", "tag2"]),
        ];

        for (input, expected) in test_cases {
            let result = NewPost::extract_hashtags(input);
            let expected: HashSet<String> = expected.into_iter().map(String::from).collect();
            assert_eq!(result, expected, "Failed for input: {}", input);
        }
    }

    #[test]
    fn test_insert_single_mention() {
        let mut buf = PostBuffer::default();
        buf.insert_text("test ", 0);
        buf.insert_text("@", 5);
        println!("{:?}", buf);
        assert!(buf.get_mention(5).is_some());
        buf.insert_text("jb55", 6);
        assert_eq!(buf.as_str(), "test @jb55");
        assert_eq!(buf.mentions.len(), 1);
        assert_eq!(buf.mentions.first().unwrap().bounds(), (5, 10));

        buf.select_full_mention(0, JB55());

        assert_eq!(
            buf.mentions.first().unwrap().mention_type,
            MentionType::Finalized(JB55())
        );
    }

    #[test]
    fn test_insert_mention_with_space() {
        let mut buf = PostBuffer::default();
        buf.insert_text("@", 0);
        buf.insert_text("jb", 1);
        buf.insert_text("55", 3);
        assert!(buf.get_mention(1).is_some());
        assert_eq!(buf.mentions.len(), 1);
        assert_eq!(buf.mentions.first().unwrap().bounds(), (0, 5));
        buf.insert_text(" test", 5);
        assert_eq!(buf.mentions.first().unwrap().bounds(), (0, 10));
        assert_eq!(buf.as_str(), "@jb55 test");

        buf.select_full_mention(0, JB55());

        assert_eq!(
            buf.mentions.first().unwrap().mention_type,
            MentionType::Finalized(JB55())
        );
    }

    #[test]
    fn test_insert_partial_to_full() {
        let mut buf = PostBuffer::default();
        buf.insert_text("@jb", 0);
        assert_eq!(buf.mentions.len(), 1);
        assert_eq!(buf.mentions.first().unwrap().bounds(), (0, 3));
        buf.select_mention_and_replace_name(0, "jb55", JB55());
        assert_eq!(buf.as_str(), "@jb55");

        buf.insert_text(" test", 5);
        assert_eq!(buf.as_str(), "@jb55 test");

        assert_eq!(buf.mentions.len(), 1);
        let mention = buf.mentions.first().unwrap();
        assert_eq!(mention.bounds(), (0, 5));
        assert_eq!(mention.mention_type, MentionType::Finalized(JB55()));
    }

    #[test]
    fn test_insert_mention_after() {
        let mut buf = PostBuffer::default();
        buf.insert_text("test text here", 0);
        buf.insert_text("@jb55", 4);

        assert!(buf.get_mention(4).is_some());
        assert_eq!(buf.mentions.len(), 1);
        assert_eq!(buf.mentions.first().unwrap().bounds(), (4, 9));
        assert_eq!("test@jb55 text here", buf.as_str());

        buf.select_full_mention(0, JB55());

        assert_eq!(
            buf.mentions.first().unwrap().mention_type,
            MentionType::Finalized(JB55())
        );
    }

    #[test]
    fn test_insert_mention_then_text() {
        let mut buf = PostBuffer::default();

        buf.insert_text("@jb55", 0);
        buf.select_full_mention(0, JB55());

        buf.insert_text(" test", 5);
        assert_eq!(buf.as_str(), "@jb55 test");
        assert_eq!(buf.mentions.len(), 1);
        assert_eq!(buf.mentions.first().unwrap().bounds(), (0, 5));
        assert!(buf.get_mention(6).is_none());
    }

    #[test]
    fn test_insert_two_mentions() {
        let mut buf = PostBuffer::default();

        buf.insert_text("@jb55", 0);
        buf.select_full_mention(0, JB55());
        buf.insert_text(" test ", 5);
        buf.insert_text("@kernelkind", 11);
        buf.select_full_mention(1, KK());
        buf.insert_text(" test", 22);

        assert_eq!(buf.as_str(), "@jb55 test @kernelkind test");
        assert_eq!(buf.mentions.len(), 2);
        let mut mentions = buf.mentions.iter();
        assert_eq!(mentions.next().unwrap().bounds(), (0, 5));
        assert_eq!(mentions.next().unwrap().bounds(), (11, 22));
    }

    #[test]
    fn test_break_mention() {
        let mut buf = PostBuffer::default();

        buf.insert_text("@jb55", 0);
        buf.select_full_mention(0, JB55());
        buf.insert_text(" test", 5);

        assert_eq!(buf.mentions.len(), 1);
        let mention = buf.mentions.first().unwrap();
        assert_eq!(mention.bounds(), (0, 5));
        assert_eq!(mention.mention_type, MentionType::Finalized(JB55()));

        buf.insert_text("oops", 2);
        assert_eq!(buf.as_str(), "@joopsb55 test");
        assert_eq!(buf.mentions.len(), 1);
        let mention = buf.mentions.first().unwrap();
        assert_eq!(mention.bounds(), (0, 9));
        assert_eq!(mention.mention_type, MentionType::Pending);
    }

    #[test]
    fn test_delete_text() {
        let mut buf = PostBuffer::default();
        buf.insert_text("hello world", 0);
        buf.delete_char_range(1..3);
        assert_eq!(buf.as_str(), "hlo world");
    }

    #[test]
    fn test_partial_delete_pending_mention() {
        let mut buf = PostBuffer::default();
        buf.insert_text("hello ", 0);
        buf.insert_text("@jb5", 6);
        assert_eq!(buf.as_str(), "hello @jb5");

        buf.delete_char_range(8..10);
        assert_eq!(buf.as_str(), "hello @j");
        assert_eq!(buf.mentions.len(), 1);
        assert_eq!(buf.mentions.first().unwrap().bounds(), (6, 8));
    }

    #[test]
    fn test_partial_delete_final_mention() {
        let mut buf = PostBuffer::default();
        buf.insert_text("hello ", 0);
        buf.insert_text("@jb55", 6);
        buf.select_full_mention(0, JB55());
        assert_eq!(buf.as_str(), "hello @jb55");
        assert_eq!(
            buf.mentions.first().unwrap().mention_type,
            MentionType::Finalized(JB55())
        );

        buf.delete_char_range(8..11);
        assert_eq!(buf.as_str(), "hello @j");
        assert_eq!(buf.mentions.len(), 1);
        let mention = buf.mentions.first().unwrap();
        assert_eq!(mention.bounds(), (6, 8));
        assert_eq!(mention.mention_type, MentionType::Pending);
    }

    #[test]
    fn test_delete_mention() {
        let mut buf = PostBuffer::default();
        buf.insert_text("hello ", 0);
        buf.insert_text("@jb55", 6);
        buf.select_full_mention(0, JB55());
        assert_eq!(buf.as_str(), "hello @jb55");
        assert_eq!(
            buf.mentions.first().unwrap().mention_type,
            MentionType::Finalized(JB55())
        );

        buf.delete_char_range(6..11);
        assert_eq!(buf.as_str(), "hello ");
        assert!(buf.mentions.is_empty());
        assert!(buf.mention_starts.is_empty());
        assert!(buf.mention_ends.is_empty());
    }

    #[test]
    fn test_two_mentions_delete_second_partial() {
        let mut buf = PostBuffer::default();

        buf.insert_text("@jb55", 0);
        buf.select_full_mention(0, JB55());
        buf.insert_text(" test ", 5);
        buf.insert_text("@kernelkind", 11);
        buf.select_full_mention(1, KK());
        buf.insert_text(" test", 22);

        assert_eq!(buf.as_str(), "@jb55 test @kernelkind test");
        buf.delete_char_range(13..22);
        assert_eq!(buf.as_str(), "@jb55 test @k test");

        assert_eq!(buf.mentions.len(), 2);
        let mut mentions = buf.mentions.iter();
        let jb_mention = mentions.next().unwrap();
        let kk_mention = mentions.next().unwrap();
        assert_eq!(jb_mention.bounds(), (0, 5));
        assert_eq!(jb_mention.mention_type, MentionType::Finalized(JB55()));
        assert_eq!(kk_mention.bounds(), (11, 13));
        assert_eq!(kk_mention.mention_type, MentionType::Pending);
    }

    #[test]
    fn test_two_mentions_delete_first_partial() {
        let mut buf = PostBuffer::default();

        buf.insert_text("@jb55", 0);
        buf.select_full_mention(0, JB55());
        buf.insert_text(" test ", 5);
        buf.insert_text("@kernelkind", 11);
        buf.select_full_mention(1, KK());
        buf.insert_text(" test", 22);

        assert_eq!(buf.as_str(), "@jb55 test @kernelkind test");
        buf.delete_char_range(3..5);
        assert_eq!(buf.as_str(), "@jb test @kernelkind test");

        assert_eq!(buf.mentions.len(), 2);
        let mut mentions = buf.mentions.iter();
        assert_eq!(mentions.next().unwrap().bounds(), (0, 3));
        assert_eq!(mentions.next().unwrap().bounds(), (9, 20));
    }

    #[test]
    fn test_two_partial_mentions() {
        let mut buf = PostBuffer::default();

        buf.insert_text("@jb", 0);
        buf.select_mention_and_replace_name(0, "jb55", JB55());
        buf.insert_text(" test ", 5);
        buf.insert_text("@kernel", 11);
        buf.select_mention_and_replace_name(1, "KernelKind", KK());
        buf.insert_text(" test", 22);

        assert_eq!(buf.as_str(), "@jb55 test @KernelKind test");
        assert_eq!(buf.mentions.len(), 2);
        let mut mentions = buf.mentions.iter();
        let jb_mention = mentions.next().unwrap();
        let kk_mention = mentions.next().unwrap();
        assert_eq!(jb_mention.bounds(), (0, 5));
        assert_eq!(jb_mention.mention_type, MentionType::Finalized(JB55()));
        assert_eq!(kk_mention.bounds(), (11, 22));
        assert_eq!(kk_mention.mention_type, MentionType::Finalized(KK()));
    }

    #[test]
    fn test_two_then_one_between() {
        let mut buf = PostBuffer::default();

        buf.insert_text("@jb", 0);
        buf.select_mention_and_replace_name(0, "jb55", JB55());
        buf.insert_text(" test ", 5);
        buf.insert_text("@kernel", 11);
        buf.select_mention_and_replace_name(1, "KernelKind", KK());
        buf.insert_text(" test", 22);

        assert_eq!(buf.as_str(), "@jb55 test @KernelKind test");
        assert_eq!(buf.mentions.len(), 2);

        buf.insert_text(" ", 5);
        buf.insert_text("@els", 6);
        assert_eq!(buf.mentions.len(), 3);
        assert_eq!(buf.mentions.get(2).unwrap().bounds(), (6, 10));
        buf.select_mention_and_replace_name(2, "elsat", JB55());
        assert_eq!(buf.as_str(), "@jb55 @elsat test @KernelKind test");

        let mut mentions = buf.mentions.iter();
        let jb_mention = mentions.next().unwrap();
        let kk_mention = mentions.next().unwrap();
        let el_mention = mentions.next().unwrap();
        assert_eq!(jb_mention.bounds(), (0, 5));
        assert_eq!(jb_mention.mention_type, MentionType::Finalized(JB55()));
        assert_eq!(kk_mention.bounds(), (18, 29));
        assert_eq!(kk_mention.mention_type, MentionType::Finalized(KK()));
        assert_eq!(el_mention.bounds(), (6, 12));
        assert_eq!(el_mention.mention_type, MentionType::Finalized(JB55()));
    }

    #[test]
    fn note_single_mention() {
        let mut buf = PostBuffer::default();
        buf.insert_text("@jb55", 0);
        buf.select_full_mention(0, JB55());

        let out = buf.output();
        let kp = FullKeypair::generate();
        let post = NewPost::new(out.text, kp.clone(), Vec::new(), out.mentions);
        let note = post.to_note(&kp.pubkey);

        let mut tags_iter = note.tags().iter();
        tags_iter.next(); //ignore the first one, the client tag
        let tag = tags_iter.next().unwrap();
        assert_eq!(tag.count(), 2);
        assert_eq!(tag.get(0).unwrap().str().unwrap(), "p");
        assert_eq!(tag.get(1).unwrap().id().unwrap(), JB55().bytes());
        assert!(tags_iter.next().is_none());
        assert_eq!(
            note.content(),
            "nostr:npub1xtscya34g58tk0z605fvr788k263gsu6cy9x0mhnm87echrgufzsevkk5s"
        );
    }

    #[test]
    fn note_two_mentions() {
        let mut buf = PostBuffer::default();

        buf.insert_text("@jb55", 0);
        buf.select_full_mention(0, JB55());
        buf.insert_text(" test ", 5);
        buf.insert_text("@KernelKind", 11);
        buf.select_full_mention(1, KK());
        buf.insert_text(" test", 22);
        assert_eq!(buf.as_str(), "@jb55 test @KernelKind test");

        let out = buf.output();
        let kp = FullKeypair::generate();
        let post = NewPost::new(out.text, kp.clone(), Vec::new(), out.mentions);
        let note = post.to_note(&kp.pubkey);

        let mut tags_iter = note.tags().iter();
        tags_iter.next(); //ignore the first one, the client tag
        let jb_tag = tags_iter.next().unwrap();
        assert_eq!(jb_tag.count(), 2);
        assert_eq!(jb_tag.get(0).unwrap().str().unwrap(), "p");
        assert_eq!(jb_tag.get(1).unwrap().id().unwrap(), JB55().bytes());

        let kk_tag = tags_iter.next().unwrap();
        assert_eq!(kk_tag.count(), 2);
        assert_eq!(kk_tag.get(0).unwrap().str().unwrap(), "p");
        assert_eq!(kk_tag.get(1).unwrap().id().unwrap(), KK().bytes());

        assert!(tags_iter.next().is_none());

        assert_eq!(note.content(), "nostr:npub1xtscya34g58tk0z605fvr788k263gsu6cy9x0mhnm87echrgufzsevkk5s test nostr:npub1fgz3pungsr2quse0fpjuk4c5m8fuyqx2d6a3ddqc4ek92h6hf9ns0mjeck test");
    }

    #[test]
    fn note_one_pending() {
        let mut buf = PostBuffer::default();

        buf.insert_text("test ", 0);
        buf.insert_text("@jb55 test", 5);

        let out = buf.output();
        let kp = FullKeypair::generate();
        let post = NewPost::new(out.text, kp.clone(), Vec::new(), out.mentions);
        let note = post.to_note(&kp.pubkey);

        let mut tags_iter = note.tags().iter();
        tags_iter.next(); //ignore the first one, the client tag
        assert!(tags_iter.next().is_none());
        assert_eq!(note.content(), "test @jb55 test");
    }
}
