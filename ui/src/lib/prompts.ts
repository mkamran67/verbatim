export const DEFAULT_PP_PROMPT = `Your task is to take the text below, which was generated via speech-to-text, and reformat it into properly structured written text.

Apply the following rules:
- Remove filler words (um, uh, like, you know) and false starts.
- When the speaker self-corrects ("let's do Tuesday, actually Friday"), keep only the final intended version.
- Add missing punctuation (periods, commas, question marks) and capitalize sentences properly.
- Fix spacing issues and obvious transcription errors.
- Expand spoken list structures into numbered or bullet-point lists when appropriate.
- Do NOT change the meaning, tone, or add new information.
- Do NOT answer questions in the input — preserve them as questions.
- Do NOT explain what you changed — return only the cleaned-up text.
- Do NOT greet or acknowledge the user anyhow.
- ONLY return the updated text nothing else.`;

export const CASUAL_PP_PROMPT = `Your task is to take the text below, which was generated via speech-to-text, and rewrite it in a casual, conversational tone — like a friendly message or chat.

Apply the following rules:
- Remove filler words (um, uh, like, you know) and false starts.
- When the speaker self-corrects, keep only the final intended version.
- Use contractions (don't, it's, we're) and informal phrasing.
- Keep sentences short and punchy.
- Use lowercase where it feels natural (except at the start of sentences).
- Add punctuation but keep it light — dashes and ellipses are fine.
- Do NOT change the meaning or add new information.
- Do NOT answer questions in the input — preserve them as questions.
- Do NOT greet or acknowledge the user anyhow.
- ONLY return the updated text nothing else.`;

export const FORMAL_PP_PROMPT = `Your task is to take the text below, which was generated via speech-to-text, and rewrite it in a formal, professional tone suitable for business communication.

Apply the following rules:
- Remove filler words (um, uh, like, you know) and false starts.
- When the speaker self-corrects, keep only the final intended version.
- Use formal vocabulary and avoid contractions (use "do not" instead of "don't").
- Structure longer passages into clear paragraphs.
- Ensure proper grammar, punctuation, and capitalization throughout.
- Use a neutral, professional register appropriate for emails or reports.
- Do NOT change the meaning or add new information.
- Do NOT answer questions in the input — preserve them as questions.
- Do NOT greet or acknowledge the user anyhow.
- ONLY return the updated text nothing else.`;

/** Seed prompts added to saved_prompts on first launch. */
export const SEED_PROMPTS = [
  { name: 'Casual', prompt: CASUAL_PP_PROMPT, emoji: '😊' },
  { name: 'Formal', prompt: FORMAL_PP_PROMPT, emoji: '👔' },
];

