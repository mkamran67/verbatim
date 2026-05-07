import { useState, useRef, useEffect } from 'react';

interface Emoji {
  emoji: string;
  keywords: string;
}

interface Category {
  name: string;
  icon: string;
  emojis: Emoji[];
}

const CATEGORIES: Category[] = [
  {
    name: 'Smileys & People',
    icon: '😊',
    emojis: [
      { emoji: '😊', keywords: 'smile happy' },
      { emoji: '😄', keywords: 'grin happy laugh' },
      { emoji: '😎', keywords: 'cool sunglasses' },
      { emoji: '🤔', keywords: 'thinking hmm' },
      { emoji: '😂', keywords: 'laugh cry tears' },
      { emoji: '🥳', keywords: 'party celebrate' },
      { emoji: '🤓', keywords: 'nerd glasses smart' },
      { emoji: '😇', keywords: 'angel halo innocent' },
      { emoji: '🤩', keywords: 'starstruck excited' },
      { emoji: '😏', keywords: 'smirk sly' },
      { emoji: '🙃', keywords: 'upside down silly' },
      { emoji: '🤗', keywords: 'hug warm friendly' },
      { emoji: '👋', keywords: 'wave hello hi' },
      { emoji: '👍', keywords: 'thumbs up good' },
      { emoji: '👏', keywords: 'clap applause' },
      { emoji: '🙌', keywords: 'raise hands celebrate' },
      { emoji: '💪', keywords: 'strong muscle flex' },
      { emoji: '🧑‍💻', keywords: 'developer coder programmer' },
      { emoji: '👨‍🏫', keywords: 'teacher professor' },
      { emoji: '👩‍⚕️', keywords: 'doctor health medical' },
    ],
  },
  {
    name: 'Clothing & Appearance',
    icon: '👖',
    emojis: [
      { emoji: '👔', keywords: 'tie formal business' },
      { emoji: '👕', keywords: 'tshirt shirt casual' },
      { emoji: '👖', keywords: 'jeans pants' },
      { emoji: '👗', keywords: 'dress fashion' },
      { emoji: '👘', keywords: 'kimono robe' },
      { emoji: '👒', keywords: 'hat sun' },
      { emoji: '🎩', keywords: 'top hat formal' },
      { emoji: '👑', keywords: 'crown king queen royal' },
      { emoji: '👓', keywords: 'glasses spectacles' },
      { emoji: '🕶️', keywords: 'sunglasses cool' },
      { emoji: '👟', keywords: 'sneaker shoe running' },
      { emoji: '👠', keywords: 'heel shoe' },
      { emoji: '👢', keywords: 'boot shoe' },
      { emoji: '🧣', keywords: 'scarf winter' },
      { emoji: '🧤', keywords: 'gloves winter' },
      { emoji: '🧥', keywords: 'coat jacket' },
      { emoji: '👜', keywords: 'handbag purse bag' },
      { emoji: '🎒', keywords: 'backpack school' },
      { emoji: '💍', keywords: 'ring diamond wedding' },
      { emoji: '💎', keywords: 'gem diamond jewel' },
    ],
  },
  {
    name: 'Music & Sound',
    icon: '🎸',
    emojis: [
      { emoji: '🎵', keywords: 'music note' },
      { emoji: '🎶', keywords: 'music notes melody' },
      { emoji: '🎤', keywords: 'microphone sing karaoke' },
      { emoji: '🎧', keywords: 'headphones audio listen' },
      { emoji: '🎸', keywords: 'guitar rock' },
      { emoji: '🎹', keywords: 'piano keyboard music' },
      { emoji: '🎺', keywords: 'trumpet horn brass' },
      { emoji: '🎷', keywords: 'saxophone jazz' },
      { emoji: '🥁', keywords: 'drum beat percussion' },
      { emoji: '🎻', keywords: 'violin fiddle string' },
      { emoji: '🪗', keywords: 'accordion' },
      { emoji: '🔔', keywords: 'bell ring notification' },
      { emoji: '🔊', keywords: 'speaker loud volume' },
      { emoji: '🔇', keywords: 'mute silent quiet' },
      { emoji: '📢', keywords: 'megaphone announce' },
      { emoji: '📣', keywords: 'cheering megaphone' },
      { emoji: '🎼', keywords: 'score sheet music' },
      { emoji: '🎙️', keywords: 'studio microphone podcast' },
    ],
  },
  {
    name: 'IT & AV',
    icon: '📱',
    emojis: [
      { emoji: '💻', keywords: 'laptop computer' },
      { emoji: '🖥️', keywords: 'desktop monitor screen' },
      { emoji: '📱', keywords: 'phone mobile smartphone' },
      { emoji: '⌨️', keywords: 'keyboard type' },
      { emoji: '🖱️', keywords: 'mouse click' },
      { emoji: '🖨️', keywords: 'printer print' },
      { emoji: '💾', keywords: 'floppy disk save' },
      { emoji: '💿', keywords: 'cd disc' },
      { emoji: '📷', keywords: 'camera photo' },
      { emoji: '📹', keywords: 'video camera record' },
      { emoji: '📺', keywords: 'tv television' },
      { emoji: '📡', keywords: 'satellite antenna signal' },
      { emoji: '🔌', keywords: 'plug electric power' },
      { emoji: '🔋', keywords: 'battery power charge' },
      { emoji: '🤖', keywords: 'robot ai bot' },
      { emoji: '🧠', keywords: 'brain smart intelligence ai' },
      { emoji: '🌐', keywords: 'globe internet web' },
      { emoji: '📶', keywords: 'signal wifi wireless' },
      { emoji: '🔒', keywords: 'lock security' },
      { emoji: '🔑', keywords: 'key password' },
    ],
  },
  {
    name: 'Office & Stationery',
    icon: '💼',
    emojis: [
      { emoji: '💼', keywords: 'briefcase work business' },
      { emoji: '📁', keywords: 'folder file' },
      { emoji: '📂', keywords: 'folder open file' },
      { emoji: '📄', keywords: 'page document' },
      { emoji: '📝', keywords: 'memo note write' },
      { emoji: '✏️', keywords: 'pencil write edit' },
      { emoji: '🖊️', keywords: 'pen write' },
      { emoji: '📎', keywords: 'paperclip attach' },
      { emoji: '📌', keywords: 'pin pushpin' },
      { emoji: '📋', keywords: 'clipboard list' },
      { emoji: '📊', keywords: 'chart graph bar stats' },
      { emoji: '📈', keywords: 'chart up growth trend' },
      { emoji: '📉', keywords: 'chart down decline' },
      { emoji: '🗂️', keywords: 'dividers index tabs' },
      { emoji: '📒', keywords: 'ledger notebook' },
      { emoji: '📕', keywords: 'book red' },
      { emoji: '📖', keywords: 'book open read' },
      { emoji: '📚', keywords: 'books library stack' },
      { emoji: '🗓️', keywords: 'calendar date schedule' },
      { emoji: '📧', keywords: 'email letter mail' },
      { emoji: '✉️', keywords: 'envelope letter mail' },
    ],
  },
  {
    name: 'Money & Time',
    icon: '💸',
    emojis: [
      { emoji: '💰', keywords: 'money bag rich' },
      { emoji: '💵', keywords: 'dollar bill cash' },
      { emoji: '💳', keywords: 'credit card payment' },
      { emoji: '💸', keywords: 'money fly spend' },
      { emoji: '🪙', keywords: 'coin currency' },
      { emoji: '💲', keywords: 'dollar sign price' },
      { emoji: '🏦', keywords: 'bank finance' },
      { emoji: '💹', keywords: 'chart yen stock' },
      { emoji: '⏰', keywords: 'alarm clock time' },
      { emoji: '⏱️', keywords: 'stopwatch timer' },
      { emoji: '⏳', keywords: 'hourglass sand time' },
      { emoji: '🕐', keywords: 'clock one time' },
      { emoji: '📅', keywords: 'calendar date' },
      { emoji: '🗒️', keywords: 'notepad spiral' },
      { emoji: '🎯', keywords: 'target bullseye goal' },
      { emoji: '🏆', keywords: 'trophy award winner' },
      { emoji: '🥇', keywords: 'gold medal first' },
      { emoji: '📆', keywords: 'calendar tear off' },
    ],
  },
  {
    name: 'Tools & Household',
    icon: '🧰',
    emojis: [
      { emoji: '🔧', keywords: 'wrench fix repair' },
      { emoji: '🔨', keywords: 'hammer build' },
      { emoji: '🪛', keywords: 'screwdriver fix' },
      { emoji: '🧰', keywords: 'toolbox tools' },
      { emoji: '⚙️', keywords: 'gear settings cog' },
      { emoji: '🔩', keywords: 'nut bolt' },
      { emoji: '🪜', keywords: 'ladder climb' },
      { emoji: '🧲', keywords: 'magnet attract' },
      { emoji: '🏠', keywords: 'house home' },
      { emoji: '🛋️', keywords: 'couch sofa living room' },
      { emoji: '🚿', keywords: 'shower bathroom' },
      { emoji: '🧹', keywords: 'broom sweep clean' },
      { emoji: '🧺', keywords: 'basket laundry' },
      { emoji: '💡', keywords: 'bulb light idea' },
      { emoji: '🔦', keywords: 'flashlight torch' },
      { emoji: '🕯️', keywords: 'candle light' },
      { emoji: '🧯', keywords: 'fire extinguisher safety' },
      { emoji: '📦', keywords: 'box package delivery' },
      { emoji: '🗑️', keywords: 'trash bin delete' },
      { emoji: '✂️', keywords: 'scissors cut' },
    ],
  },
  {
    name: 'Nature & Weather',
    icon: '🌿',
    emojis: [
      { emoji: '🌟', keywords: 'star glow shine' },
      { emoji: '✨', keywords: 'sparkle magic' },
      { emoji: '⚡', keywords: 'lightning bolt energy' },
      { emoji: '🔥', keywords: 'fire hot flame' },
      { emoji: '🌈', keywords: 'rainbow' },
      { emoji: '☀️', keywords: 'sun sunny bright' },
      { emoji: '🌙', keywords: 'moon night' },
      { emoji: '⭐', keywords: 'star' },
      { emoji: '🌊', keywords: 'wave ocean water' },
      { emoji: '🌿', keywords: 'leaf herb nature' },
      { emoji: '🌸', keywords: 'cherry blossom flower' },
      { emoji: '🌺', keywords: 'hibiscus flower' },
      { emoji: '🍀', keywords: 'clover luck four leaf' },
      { emoji: '🌍', keywords: 'earth globe world' },
      { emoji: '❄️', keywords: 'snowflake cold winter' },
      { emoji: '🌪️', keywords: 'tornado storm' },
      { emoji: '☁️', keywords: 'cloud weather' },
      { emoji: '🦋', keywords: 'butterfly' },
    ],
  },
  {
    name: 'Food & Drink',
    icon: '☕',
    emojis: [
      { emoji: '☕', keywords: 'coffee hot drink' },
      { emoji: '🍵', keywords: 'tea cup green' },
      { emoji: '🧃', keywords: 'juice box' },
      { emoji: '🍕', keywords: 'pizza' },
      { emoji: '🍔', keywords: 'hamburger burger' },
      { emoji: '🌮', keywords: 'taco' },
      { emoji: '🍜', keywords: 'noodle ramen soup' },
      { emoji: '🍰', keywords: 'cake dessert sweet' },
      { emoji: '🍎', keywords: 'apple fruit red' },
      { emoji: '🍋', keywords: 'lemon citrus' },
      { emoji: '🥑', keywords: 'avocado' },
      { emoji: '🧁', keywords: 'cupcake sweet' },
      { emoji: '🍩', keywords: 'donut doughnut sweet' },
      { emoji: '🥂', keywords: 'champagne cheers toast' },
      { emoji: '🍷', keywords: 'wine glass red' },
      { emoji: '🍺', keywords: 'beer mug' },
    ],
  },
];

interface EmojiPickerProps {
  value: string;
  onChange: (emoji: string) => void;
}

export default function EmojiPicker({ value, onChange }: EmojiPickerProps) {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState('');
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handle = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener('mousedown', handle);
    return () => window.removeEventListener('mousedown', handle);
  }, [open]);

  useEffect(() => {
    if (!open) setSearch('');
  }, [open]);

  const query = search.toLowerCase().trim();

  const filteredCategories = query
    ? CATEGORIES.map((cat) => ({
        ...cat,
        emojis: cat.emojis.filter((e) =>
          e.keywords.includes(query) || e.emoji === query
        ),
      })).filter((cat) => cat.emojis.length > 0)
    : CATEGORIES;

  return (
    <div ref={ref} className="relative">
      {/* Trigger button */}
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className="w-12 h-12 rounded-xl border-2 border-slate-200 dark:border-slate-600 bg-slate-50 dark:bg-slate-700 hover:border-amber-300 dark:hover:border-amber-500 flex items-center justify-center text-2xl cursor-pointer transition-all"
      >
        {value}
      </button>

      {/* Picker dropdown */}
      {open && (
        <div className="absolute z-50 mt-2 right-0 w-80 max-h-96 bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-600 shadow-xl flex flex-col overflow-hidden">
          {/* Search */}
          <div className="px-3 pt-3 pb-2 border-b border-slate-100 dark:border-slate-700">
            <div className="flex items-center gap-2 bg-slate-50 dark:bg-slate-700 border border-slate-200 dark:border-slate-600 rounded-lg px-2.5 py-1.5">
              <i className="ri-search-line text-slate-400 text-sm" />
              <input
                autoFocus
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="Search emojis..."
                className="bg-transparent text-sm text-slate-700 dark:text-slate-300 outline-none flex-1 placeholder:text-slate-400 dark:placeholder:text-slate-500"
              />
            </div>
          </div>

          {/* Emoji grid */}
          <div className="flex-1 overflow-y-auto px-3 py-2">
            {filteredCategories.length === 0 ? (
              <p className="text-slate-400 dark:text-slate-500 text-xs text-center py-6">No emojis found</p>
            ) : (
              filteredCategories.map((cat) => (
                <div key={cat.name} className="mb-3 last:mb-1">
                  <p className="text-[10px] font-semibold text-slate-400 dark:text-slate-500 uppercase tracking-widest mb-1.5 flex items-center gap-1.5">
                    <span>{cat.icon}</span> {cat.name}
                  </p>
                  <div className="grid grid-cols-8 gap-0.5">
                    {cat.emojis.map((e) => (
                      <button
                        key={e.emoji}
                        type="button"
                        onClick={() => {
                          onChange(e.emoji);
                          setOpen(false);
                        }}
                        className={`w-8 h-8 flex items-center justify-center rounded-lg text-lg cursor-pointer transition-all ${
                          value === e.emoji
                            ? 'bg-amber-100 dark:bg-amber-500/20 ring-2 ring-amber-400'
                            : 'hover:bg-slate-100 dark:hover:bg-slate-700'
                        }`}
                      >
                        {e.emoji}
                      </button>
                    ))}
                  </div>
                </div>
              ))
            )}
          </div>
        </div>
      )}
    </div>
  );
}
