use std::collections::BTreeMap;

/// Only user-authored text and the expected amend target belong in draft storage.
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub(super) struct CommitDraft {
    pub(super) message: String,
    pub(super) amend_head: Option<String>,
    pub(super) normal_message: Option<String>,
}

#[derive(Default)]
pub struct CommitState {
    pub message: String,
    pub contents: crate::git::CommitContents,
    pub last_generation_tokens: Option<crate::llm::TokenUsage>,
    pub session_generation_tokens: u64,
    pub generating_message: bool,
    pub(super) generation_cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    pub amend: bool,
    pub(super) merge_default_message: Option<String>,
    pub(super) amend_head: Option<String>,
    pub(super) amend_draft: Option<String>,
    pub(super) commit_drafts: BTreeMap<String, CommitDraft>,
}

impl CommitState {
    pub(super) fn finish_commit(&mut self) {
        self.message = self.amend_draft.take().unwrap_or_default();
        self.amend = false;
        self.amend_head = None;
    }

    pub(super) fn begin_amend(&mut self, head: String, message: String) {
        self.amend_draft = Some(std::mem::replace(&mut self.message, message));
        self.amend_head = Some(head);
        self.amend = true;
    }

    pub(super) fn cancel_amend(&mut self) {
        self.amend = false;
        self.amend_head = None;
        if let Some(draft) = self.amend_draft.take() {
            self.message = draft;
        }
    }

    pub(super) fn remember(&mut self, path: &str) {
        let draft = CommitDraft {
            message: if self.merge_default_message.as_ref() == Some(&self.message) {
                String::new()
            } else {
                self.message.clone()
            },
            amend_head: self.amend_head.clone().filter(|_| self.amend),
            normal_message: self.amend_draft.clone().filter(|_| self.amend),
        };
        if draft.message.is_empty()
            && draft
                .normal_message
                .as_deref()
                .unwrap_or_default()
                .is_empty()
        {
            self.commit_drafts.remove(path);
        } else {
            self.commit_drafts.insert(path.to_owned(), draft);
        }
    }
    pub(super) fn load(&mut self, path: Option<&str>) {
        let draft = path
            .and_then(|p| self.commit_drafts.get(p))
            .cloned()
            .unwrap_or_default();
        self.amend_head = draft.amend_head;
        self.amend_draft = draft.normal_message;
        self.amend = self.amend_head.is_some();
        self.message = draft.message;
    }
}
