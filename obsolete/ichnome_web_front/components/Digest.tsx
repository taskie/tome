type Props = {
  digest: string;
  length?: number;
};

export const Digest: React.FC<Props> = ({ digest, length }) => (
  <code style={{ fontSize: "0.9rem" }}>{length != null ? digest.slice(0, length) : digest}</code>
);

export default Digest;
