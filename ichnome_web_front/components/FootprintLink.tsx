import Link from "next/link";
import { uri } from "../utils/uri";
import Digest from "./Digest";

type Props = {
  workspaceName: string;
  digest: string;
  length?: number;
};

export const FootprintLink: React.FC<Props> = ({ workspaceName, digest, length }) => (
  <Link href="/[workspaceName]/footprints/[digest]" as={uri`/${workspaceName}/footprints/${digest}`}>
    <a>
      <Digest digest={digest} length={length} />
    </a>
  </Link>
);

export default FootprintLink;
