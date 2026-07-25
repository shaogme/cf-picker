use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use hashbrown::HashSet;
use regex::Regex;

use crate::{hash::FastBuildHasher, traits::Rand};

#[derive(Debug, Clone)]
pub struct ColoMatcher {
    re_iata: Regex,
    re_country: Regex,
    re_gcore: Regex,
    target_colos: Option<HashSet<String, FastBuildHasher>>,
}

impl ColoMatcher {
    pub fn new<G: Rand + ?Sized>(target_colos_str: &str, rng: &mut G) -> Self {
        Self::new_with_hasher(target_colos_str, FastBuildHasher::from_rng(rng))
    }

    pub fn new_with_hasher(target_colos_str: &str, hasher: FastBuildHasher) -> Self {
        let target_colos = if target_colos_str.trim().is_empty() {
            None
        } else {
            let mut set = HashSet::with_hasher(hasher);
            for s in target_colos_str.split(',') {
                let trimmed = s.trim().to_uppercase();
                if !trimmed.is_empty() {
                    set.insert(trimmed);
                }
            }
            Some(set)
        };

        Self {
            re_iata: Regex::new(r"[A-Z]{3}").expect("valid regex"),
            re_country: Regex::new(r"[A-Z]{2}").expect("valid regex"),
            re_gcore: Regex::new(r"^[a-z]{2}").expect("valid regex"),
            target_colos,
        }
    }

    pub fn extract_colo<R: crate::traits::HttpingResponse + ?Sized>(&self, resp: &R) -> String {
        if let Some(server) = resp.header("server") {
            if server == "cloudflare"
                && let Some(cf_ray) = resp.header("cf-ray")
                && let Some(m) = self.re_iata.find(cf_ray)
            {
                return m.as_str().to_string();
            }
            if server == "CDN77-Turbo"
                && let Some(pop) = resp.header("x-77-pop")
                && let Some(m) = self.re_country.find(pop)
            {
                return m.as_str().to_string();
            }
            if server.contains("BunnyCDN-") {
                let stripped = server.trim_start_matches("BunnyCDN-");
                if let Some(m) = self.re_country.find(stripped) {
                    return m.as_str().to_string();
                }
            }
        }

        if let Some(pop) = resp.header("x-amz-cf-pop")
            && let Some(m) = self.re_iata.find(pop)
        {
            return m.as_str().to_string();
        }

        if let Some(served_by) = resp.header("x-served-by") {
            let matches: Vec<_> = self.re_iata.find_iter(served_by).collect();
            if let Some(last) = matches.last() {
                return last.as_str().to_string();
            }
        }

        if let Some(id_fe) = resp.header("x-id-fe")
            && let Some(m) = self.re_gcore.find(id_fe)
        {
            return m.as_str().to_uppercase();
        }

        String::new()
    }

    pub fn is_match(&self, colo: &str) -> bool {
        if colo.is_empty() {
            return false;
        }
        match &self.target_colos {
            Some(set) => set.contains(&colo.to_uppercase()),
            None => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::HttpingResponse;

    struct DummyResponse {
        server: String,
        cf_ray: String,
    }

    impl HttpingResponse for DummyResponse {
        fn status(&self) -> u16 {
            200
        }

        fn header(&self, name: &str) -> Option<&str> {
            match name {
                "server" => Some(&self.server),
                "cf-ray" => Some(&self.cf_ray),
                _ => None,
            }
        }
    }

    struct MockRand;
    impl Rand for MockRand {
        fn next_u64(&mut self) -> u64 {
            12345
        }
    }

    #[test]
    fn test_extract_cloudflare_colo() {
        let mut rng = MockRand;
        let matcher = ColoMatcher::new("HKG,SJC", &mut rng);
        let resp = DummyResponse {
            server: "cloudflare".to_string(),
            cf_ray: "7bd32409eda7b020-SJC".to_string(),
        };

        let colo = matcher.extract_colo(&resp);
        assert_eq!(colo, "SJC");
        assert!(matcher.is_match(&colo));
    }
}
