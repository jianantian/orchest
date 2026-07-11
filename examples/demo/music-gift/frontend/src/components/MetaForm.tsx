import { useRef, useState } from 'react';
import type { GiftMeta } from '../types';
import { uploadPhotos } from '../api';

interface MetaFormProps {
  meta: GiftMeta;
  setMeta: (meta: GiftMeta) => void;
  photos: string[];
  setPhotos: (photos: string[]) => void;
}

const RELATIONSHIPS = ['Friend', 'Partner', 'Parent', 'Sibling', 'Child', 'Other'];
const LANGS = [
  { value: 'en', label: 'English' },
  { value: 'zh', label: '中文' },
  { value: 'fr', label: 'Français' },
  { value: 'es', label: 'Español' },
  { value: 'ru', label: 'Русский' },
];

export default function MetaForm({ meta, setMeta, photos, setPhotos }: MetaFormProps) {
  const fileRef = useRef<HTMLInputElement>(null);
  const [uploading, setUploading] = useState(false);
  const [previewUrls, setPreviewUrls] = useState<string[]>([]);

  function update<K extends keyof GiftMeta>(key: K, value: GiftMeta[K]) {
    setMeta({ ...meta, [key]: value });
  }

  async function handlePhotoSelect(e: React.ChangeEvent<HTMLInputElement>) {
    const files = e.target.files;
    if (!files || files.length === 0) return;

    setUploading(true);
    try {
      const fileArr = Array.from(files).slice(0, 5 - photos.length);
      const dataUrls = await Promise.all(
        fileArr.map((f) => {
          return new Promise<string>((resolve, reject) => {
            const reader = new FileReader();
            reader.onload = () => resolve(reader.result as string);
            reader.onerror = reject;
            reader.readAsDataURL(f);
          });
        }),
      );

      const res = await uploadPhotos(dataUrls);
      setPhotos([...photos, ...res.urls]);
      setPreviewUrls([...previewUrls, ...dataUrls]);
    } catch {
      // Ignore upload errors - photos are optional
    } finally {
      setUploading(false);
      if (fileRef.current) fileRef.current.value = '';
    }
  }

  function removePhoto(idx: number) {
    setPhotos(photos.filter((_, i) => i !== idx));
    setPreviewUrls(previewUrls.filter((_, i) => i !== idx));
  }

  return (
    <div className="card meta-form">
      <div className="form-row">
        <div className="form-group">
          <label className="form-label">Recipient Name</label>
          <input
            className="form-input"
            type="text"
            value={meta.name ?? ''}
            onChange={(e) => update('name', e.target.value)}
            placeholder="Who is this for?"
          />
        </div>
        <div className="form-group">
          <label className="form-label">Relationship</label>
          <select
            className="form-select"
            value={meta.relationship ?? ''}
            onChange={(e) => update('relationship', e.target.value)}
          >
            <option value="">Select…</option>
            {RELATIONSHIPS.map((r) => (
              <option key={r} value={r}>
                {r}
              </option>
            ))}
          </select>
        </div>
      </div>

      <div className="form-group">
        <label className="form-label">Scenario / Memory</label>
        <textarea
          className="form-textarea"
          value={meta.scenario ?? ''}
          onChange={(e) => update('scenario', e.target.value)}
          placeholder="A special moment, a shared memory, or what makes them unique…"
          rows={3}
        />
      </div>

      <div className="form-group">
        <label className="form-label">Language</label>
        <select
          className="form-select"
          value={meta.lang ?? 'en'}
          onChange={(e) => update('lang', e.target.value)}
        >
          {LANGS.map((l) => (
            <option key={l.value} value={l.value}>
              {l.label}
            </option>
          ))}
        </select>
      </div>

      <div className="form-group">
        <label className="form-label">Photos (optional, up to 5)</label>
        <div className="photo-upload-area">
          {previewUrls.map((url, i) => (
            <div key={i} className="photo-thumb">
              <img src={url} alt="" />
              <button className="photo-remove" onClick={() => removePhoto(i)} aria-label="Remove photo">
                ×
              </button>
            </div>
          ))}
          {photos.length < 5 && (
            <button
              className="photo-add"
              onClick={() => fileRef.current?.click()}
              disabled={uploading}
            >
              {uploading ? <span className="spinner" /> : '+'}
            </button>
          )}
          <input
            ref={fileRef}
            type="file"
            accept="image/jpeg,image/png,image/webp"
            multiple
            onChange={(e) => void handlePhotoSelect(e)}
            style={{ display: 'none' }}
          />
        </div>
      </div>
    </div>
  );
}
