export interface ClaimRecord {
  schemaVersion: number;
  subjectVersion: string;
  assessorRole: string;
  assessorChild: string;
  acceptedAt: number | null;
  supported: boolean;
  objections: string[];
  claims: {
    subject: number;
    target: string;
    claim: string;
    scope: string;
    kind: string | null;
    verdict: string;
    required: string[];
    meetsRequirements: boolean;
    objections: string[];
    comparison?: {
      reported: number;
      reproduced: number;
      decimalPlaces: number;
    } | null;
    evidence: {
      role: string;
      evidenceId: string;
      location: string;
      observed: string;
      origin: string;
      digest: string;
    }[];
  }[];
}
