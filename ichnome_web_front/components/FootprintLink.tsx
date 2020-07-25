import Link from "next/link";
import { uri } from "../utils/uri";
import Digest from "./Digest";

type Props = {
  digest: string;
  length?: number;
};

export const FootprintLink: React.FC<Props> = ({ digest, length }) => (
  <Link href={uri`/footprints/${digest}`}>
    <a>
      <Digest digest={digest} length={length} />
    </a>
  </Link>
);

export default FootprintLink;
