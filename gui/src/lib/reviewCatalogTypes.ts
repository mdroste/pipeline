export interface AutoReviewCatalogRole {
  id: string;
  label: string;
  /** "discipline" and "family" mark a group's broad fallback role. */
  level: "discipline" | "subfield" | "family" | "method";
  description: string;
  exclusions: string;
}

/** A discipline of subject roles or a family of method roles. */
export interface AutoReviewCatalogGroup {
  id: string;
  label: string;
  roles: AutoReviewCatalogRole[];
}

export interface AutoReviewCatalog {
  contract: string;
  revision: string;
  subjectCount: number;
  methodCount: number;
  genreCount: number;
  disciplines: AutoReviewCatalogGroup[];
  methodFamilies: AutoReviewCatalogGroup[];
}
