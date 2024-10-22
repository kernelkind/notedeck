use tracing::error;

use crate::{storage::FileDirectoryInteractor, FileWriterFactory};

pub struct Support {
    interactor: Option<FileDirectoryInteractor>,
    mailto_url: String,
    log_dir: Option<String>,
    most_recent_log: Option<String>,
}

impl Default for Support {
    fn default() -> Self {
        let interactor = FileWriterFactory::new(crate::FileWriterType::Log).build();
        if let Err(e) = &interactor {
            error!(
                "Support could not initialize directory interactor: {}",
                e.to_string()
            );
        }
        let interactor = interactor.ok();

        let log_dir = interactor
            .as_ref()
            .map(|interactor| format!("{:?}", interactor.get_directory()));

        Self {
            mailto_url: MailtoFactory::new(SUPPORT_EMAIL.to_string())
                .with_subject("Help Needed".to_owned())
                .with_content(EMAIL_TEMPLATE.to_owned())
                .build(),
            interactor,
            log_dir,
            most_recent_log: None,
        }
    }
}

static MAX_LOG_LINES: usize = 500;
static SUPPORT_EMAIL: &str = "support@damus.io";
static EMAIL_TEMPLATE: &str = "Describe the bug you have encountered:\n<-- your statement here -->\n\n===== Paste your log below =====\n\n";

impl Support {
    pub fn refresh(&mut self) {
        if let Some(interactor) = &self.interactor {
            self.most_recent_log = get_log_str(interactor);
        } else {
            self.interactor = FileWriterFactory::new(crate::FileWriterType::Log)
                .build()
                .ok();
        }
    }

    pub fn get_mailto_url(&self) -> &str {
        &self.mailto_url
    }

    pub fn get_log_dir(&self) -> Option<&String> {
        self.log_dir.as_ref()
    }

    pub fn get_most_recent_log(&self) -> Option<&String> {
        self.most_recent_log.as_ref()
    }
}

fn get_log_str(interactor: &FileDirectoryInteractor) -> Option<String> {
    match interactor.get_most_recent() {
        Ok(Some(most_recent_name)) => {
            match interactor.get_file_last_n_lines(most_recent_name.clone(), MAX_LOG_LINES) {
                Ok(file_output) => {
                    return Some(
                        get_prefix(
                            &most_recent_name,
                            file_output.output_num_lines,
                            file_output.total_lines_in_file,
                        ) + &file_output.output,
                    )
                }
                Err(e) => {
                    error!(
                        "Error retrieving the last lines from file {}: {:?}",
                        most_recent_name, e
                    );
                }
            }
        }
        Ok(None) => {
            error!("No files were found.");
        }
        Err(e) => {
            error!("Error fetching the most recent file: {:?}", e);
        }
    }

    None
}

fn get_prefix(file_name: &str, lines_displayed: usize, num_total_lines: usize) -> String {
    format!(
        "===\nDisplaying the last {} of {} lines in file {}\n===\n\n",
        lines_displayed, num_total_lines, file_name,
    )
}

struct MailtoFactory {
    content: Option<String>,
    address: String,
    subject: Option<String>,
    max_size: Option<usize>,
}

impl MailtoFactory {
    fn new(address: String) -> Self {
        Self {
            content: None,
            address,
            subject: None,
            max_size: None,
        }
    }

    // will be truncated so the whole URL is at most 2000 characters
    pub fn with_content(mut self, content: String) -> Self {
        self.content = Some(content);
        self
    }

    pub fn with_subject(mut self, subject: String) -> Self {
        self.subject = Some(subject);
        self
    }

    #[allow(dead_code)]
    pub fn with_capacity(mut self, max_size: usize) -> Self {
        self.max_size = Some(max_size);
        self
    }

    pub fn build(self) -> String {
        let mut url = match self.max_size {
            Some(max_size) => String::with_capacity(max_size),
            None => String::new(),
        };

        url.push_str("mailto:");
        url.push_str(&self.address);

        let has_subject = self.subject.is_some();

        if has_subject || self.content.is_some() {
            url.push('?');
        }

        if let Some(subject) = self.subject {
            url.push_str("subject=");
            url.push_str(&urlencoding::encode(&subject));
        }

        if let Some(content) = self.content {
            if has_subject {
                url.push('&');
            }

            url.push_str("body=");

            let body = urlencoding::encode(&content);

            if let Some(max_size) = self.max_size {
                if body.len() + url.len() <= max_size {
                    url.push_str(&body);
                } else {
                    url.push_str(&body[..max_size - url.len()]);
                }
            } else {
                url.push_str(&body);
            }
        }

        url
    }
}
