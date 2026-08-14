//! Portfolio domain state and commands.

use crate::session::{PortfolioSnapshot, ProjectSnapshot, SessionSnapshot};
use serde::{Deserialize, Serialize};

/// Application destinations known to the domain.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum View {
    #[default]
    Home,
    About,
    Projects,
    Contact,
    EditPortfolio,
    Calculator,
    TextAnalyzer,
    ColorConverter,
}

/// Editable portfolio content.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Portfolio {
    pub display_name: String,
    pub headline: String,
    pub about: String,
    pub projects: Vec<Project>,
    pub email: String,
    pub website: String,
    pub github: String,
}

/// Editable project content.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Project {
    pub title: String,
    pub summary: String,
    pub url: String,
}

/// Portfolio fields accepted by the central command enum.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortfolioField {
    DisplayName,
    Headline,
    About,
    Email,
    Website,
    Github,
}

/// Project fields accepted by the central command enum.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectField {
    Title,
    Summary,
    Url,
}

impl Portfolio {
    /// Sets one editable portfolio field.
    pub fn set_field(&mut self, field: PortfolioField, value: String) {
        match field {
            PortfolioField::DisplayName => self.display_name = value,
            PortfolioField::Headline => self.headline = value,
            PortfolioField::About => self.about = value,
            PortfolioField::Email => self.email = value,
            PortfolioField::Website => self.website = value,
            PortfolioField::Github => self.github = value,
        }
    }

    /// Adds one empty project.
    pub fn add_project(&mut self) {
        self.projects.push(Project::default());
    }

    /// Sets one editable project field.
    /// # Errors
    /// Returns `ProjectNotFound` when the project index is outside the collection.
    pub fn set_project_field(
        &mut self,
        index: usize,
        field: ProjectField,
        value: String,
    ) -> Result<(), PortfolioError> {
        let project = self
            .projects
            .get_mut(index)
            .ok_or(PortfolioError::ProjectNotFound { index })?;
        match field {
            ProjectField::Title => project.title = value,
            ProjectField::Summary => project.summary = value,
            ProjectField::Url => project.url = value,
        }
        Ok(())
    }

    /// Converts domain state to the framework-independent persisted form.
    #[must_use]
    pub fn snapshot(&self) -> PortfolioSnapshot {
        PortfolioSnapshot {
            display_name: self.display_name.clone(),
            headline: self.headline.clone(),
            about: self.about.clone(),
            projects: self.projects.iter().map(Project::snapshot).collect(),
            email: self.email.clone(),
            website: self.website.clone(),
            github: self.github.clone(),
        }
    }

    /// Restores domain state from the framework-independent persisted form.
    #[must_use]
    pub fn from_snapshot(snapshot: PortfolioSnapshot) -> Self {
        Self {
            display_name: snapshot.display_name,
            headline: snapshot.headline,
            about: snapshot.about,
            projects: snapshot
                .projects
                .into_iter()
                .map(Project::from_snapshot)
                .collect(),
            email: snapshot.email,
            website: snapshot.website,
            github: snapshot.github,
        }
    }
}

impl Project {
    fn snapshot(&self) -> ProjectSnapshot {
        ProjectSnapshot {
            title: self.title.clone(),
            summary: self.summary.clone(),
            url: self.url.clone(),
        }
    }

    fn from_snapshot(snapshot: ProjectSnapshot) -> Self {
        Self {
            title: snapshot.title,
            summary: snapshot.summary,
            url: snapshot.url,
        }
    }
}

/// Validation and transition failures for portfolio commands.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PortfolioError {
    ProjectNotFound { index: usize },
}

impl From<Portfolio> for PortfolioSnapshot {
    fn from(value: Portfolio) -> Self {
        value.snapshot()
    }
}

impl From<PortfolioSnapshot> for Portfolio {
    fn from(value: PortfolioSnapshot) -> Self {
        Self::from_snapshot(value)
    }
}

/// Checks the URL format accepted by portfolio links.
#[must_use]
pub fn is_valid_url(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

/// Checks the minimum email shape accepted by the portfolio.
#[must_use]
pub fn is_valid_email(email: &str) -> bool {
    email.contains('@')
}

/// Returns the portfolio portion of a persisted application session.
#[must_use]
pub fn portfolio_from_session(snapshot: &SessionSnapshot) -> Portfolio {
    Portfolio::from_snapshot(snapshot.portfolio.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_project_and_update_fields_are_domain_transitions() {
        let mut portfolio = Portfolio::default();
        portfolio.add_project();
        portfolio
            .set_project_field(0, ProjectField::Title, "Example".to_owned())
            .expect("updating an existing project must succeed");
        assert_eq!(portfolio.projects[0].title, "Example");
    }

    #[test]
    fn missing_project_is_reported_without_mutation() {
        let mut portfolio = Portfolio::default();
        let error = portfolio
            .set_project_field(0, ProjectField::Url, "https://example.test".to_owned())
            .expect_err("missing project must fail");
        assert_eq!(error, PortfolioError::ProjectNotFound { index: 0 });
    }

    #[test]
    fn portfolio_round_trips_through_session_snapshot() {
        let portfolio = Portfolio {
            display_name: "Ada".to_owned(),
            projects: vec![Project {
                title: "Calculator".to_owned(),
                ..Project::default()
            }],
            ..Portfolio::default()
        };
        let restored = Portfolio::from_snapshot(portfolio.snapshot());
        assert_eq!(restored, portfolio);
    }
}
