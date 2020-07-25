import Link from "next/link";
import { uri } from "../utils/uri";

type Props = {
  groupId: string;
  path: string;
};

export const StatLink: React.FC<Props> = ({ groupId, path }) => (
  <Link href={uri`/stats/${groupId}/` + path}>
    <a>{path}</a>
  </Link>
);

export default StatLink;
